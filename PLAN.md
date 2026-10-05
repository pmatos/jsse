# Plan: issue #858 — `with` wrapping `await using` still drains disposal inline

## 1. Problem restated

In a plain (non-generator) async function, a `with (obj) { ... }` whose body
directly declares `await using` (or `using`) is correctly detected as needing
full state-machine lowering (`scan_await_using`'s `Statement::With` arm forces
`Blocked` for anything reachable inside a `with`, so the block always goes
through `transform_scope_block`'s `EnterScope`/`ExitScope` terminators, never
the separate `Isolatable`/`suspendable_dispose_block` fast path). But
`EnterScope`'s runtime handler (`eval.rs:9750`) builds the block's own
environment (`scope_env`) as a plain child of `term_env`, with no knowledge of
the enclosing `with` at all — `with_scopes` is a `TransformContext`-local,
compile-time-only list consumed by re-wrapping each state's *statement body*
in a fresh `Statement::With(Block(...))` AST node at
`finalize_current_state` (`generator_transform.rs:469-482`). The state that
carries the `EnterScope` terminator is finalized with an empty statement list
(nothing queued yet at that point), so it never gets this re-wrap, and the
*following* state — the one whose body is the actual `await using`
declaration — does get wrapped, in a **new**, independent `Statement::With`
node. Tree-walking that node creates a fresh with-environment *and* a fresh
block-declarative-environment, and the declaration's `AddDisposableResource`
call (proposal-explicit-resource-management) registers the resource on that
throwaway block environment, not on `scope_env`. `ExitScope`'s
`take_dispose_stack(&scope_env)` then finds nothing, and disposal instead runs
through the ordinary tree-walker block-exit path when that throwaway block
closes — synchronously/inline, reproducing the "drains inline instead of
suspending" symptom by a different route than the one #856 (referenced in the
issue as "#857") fixed for `for-of` heads.

The fix must make the `with`'s object-environment part of `scope_env`'s own
permanent ancestor chain (built once, by `EnterScope`'s runtime handler, from
state saved on the terminator at transform time), not a per-state AST
rewrap — so the `await using` declaration binds directly into `scope_env`,
exactly where `ExitScope` looks for it.

## 2. Spec basis

- **`sec-with-statement-runtime-semantics-evaluation`** (§14.11 WithStatement
  Evaluation): `with (Expression) Statement` evaluates `Expression`, calls
  `ToObject` on it, then builds `newEnv = NewObjectEnvironment(obj, true,
  oldEnv)` and runs `Statement` with `newEnv` as the LexicalEnvironment —
  `oldEnv` is whatever was active at the `with`, and `newEnv` is a *single*
  persistent environment for the statement's entire (possibly
  suspension-crossing) dynamic extent, not one rebuilt per fragment. This is
  the shape `scope_env`'s ancestor chain must reproduce.
- **`sec-newobjectenvironment`** (abstract operation): defines the Object
  Environment Record this produces, `[[IsWithEnvironment]] = true`,
  `[[OuterEnv]]` = the environment active when the `with` was entered. Fixes
  the exact parent/child relationship: with-environment's outer is the
  pre-`with` environment, and (per `sec-with-statement-runtime-semantics-evaluation`
  step 5) `Statement` runs with this with-environment as *its own* direct
  LexicalEnvironment — so for a `with`-body that is itself a block declaring
  `await using`, the block's own environment is a **child of** the
  with-environment, not a sibling built independently.
- **`sec-blockdeclarationinstantiation`** / **`sec-block-runtime-semantics-evaluation`**
  (already cited by `ScopeAction::OpenBlock`'s doc comment, `generator_transform.rs:53`):
  general block-scope mechanics; confirms `EnterScope`'s `scope_env` is this
  same per-spec block environment, just routed through a different lowering
  path because it owns disposable resources.
- **Explicit Resource Management proposal** (not present in the pinned
  `spec/` commit `270a490b`; `dispose.rs:27` already cites it the same way):
  `AddDisposableResource` records a resource against "the running execution
  context's LexicalEnvironment" at the point of declaration — this must be
  `scope_env` itself (no extra lexical layer in between), and `DisposeResources`
  (`sec-disposeresources`) is what `ExitScope`'s `take_dispose_stack(&frame.env)`
  drives. The current bug is exactly that the LexicalEnvironment active when
  the declaration runs is *not* `scope_env`.

No new JavaScript syntax or semantics are introduced — this restores
conformance to clauses the engine already implements for `with` and for
`await using` individually; the bug is specific to their composition in the
internal state-machine lowering.

## 3. Files to touch

- `src/interpreter/generator_transform.rs`
  - `StateTerminator::EnterScope` (around line 251): add a `with_vars: Vec<String>` field.
  - `transform_scope_block` (line 1032): capture the `with` chain active at
    the point the scope opens, bake it onto the `EnterScope` terminator, and
    suppress the per-state AST with-rewrap for the scope's own interior (see
    slice 2 below for the precise ordering).
  - `clear_terminator_ic_sites` (line 286): no change — `EnterScope { .. }`
    already matches by wildcard and `with_vars` holds plain temp-var names,
    not expressions.
- `src/interpreter/types.rs`
  - Add `Environment::new_with_object(parent: EnvRef, obj_id: ObjectId) -> EnvRef`
    (or equivalent), factored out of the inline struct literal at
    `exec.rs:1212-1231`, so both the direct tree-walker's `Statement::With`
    handling and the new `EnterScope` runtime handler build an identical
    with-environment instead of duplicating the ~15-field literal.
- `src/interpreter/exec.rs`
  - `Statement::With` handling (line 1198): switch to the new
    `Environment::new_with_object` constructor (pure deduplication, no
    behavior change — needed because the fix below introduces the second
    call site).
- `src/interpreter/eval.rs`
  - `StateTerminator::EnterScope { body_state }` handling (line 9750):
    destructure `with_vars` too; for each with-var (outermost first), read
    its current value out of `term_env`, `ToObject` it (propagate a throw via
    `pending_exception`/`continue`, matching the pattern used elsewhere in
    this match), and chain `Environment::new_with_object` calls before
    creating `scope_env` as the innermost child. Set
    `self.has_ever_entered_with = true` when `with_vars` is non-empty (see
    slice 2 and Regression risk — `with_scope_depth` is deliberately *not*
    touched here).
- `test262-extra/` — new regression tests (slice 1, 3, 4, 5 below).
- No `src/bytecode/` changes: the bytecode compiler bails out of `with` and
  of `await using` today (tree-walker-only constructs), so this path is
  unreached there.
- No `docs/adr/` entry: this is a bug fix restoring existing documented
  behavior (the `EnterScope`/`ExitScope` doc comments already describe the
  intended invariant), not a new architectural decision.

## 4. TDD slices

1. **Red: reproduce the issue's exact repro.**
   Add `test262-extra/with-wraps-await-using-block-dispose-tick-alignment.js`,
   following the tick-alignment pattern already used by
   `test262-extra/await-using-for-of-head-wraps-await-using-block-dispose-tick-alignment.js`
   (`asyncTest`, `observe()`'s witness-promise chain, `compareArray`). Encode
   the issue's repro: `with ({}) { await using a = { [Symbol.asyncDispose](){...} }; }`
   inside a plain async function, asserting the disposal suspends (lands on a
   later promise-reaction tick, same as Node) rather than draining before
   `'sync-end'`. `flags: [async, noStrict]` — `with` is a `SyntaxError` in
   strict mode, and the default test262 runner also runs strict-mode
   variants, so `noStrict` is required or the harness will flag a spurious
   failure. This test fails today with the current tick order from the issue
   body (`disp-async`, `after`, `sync-end` instead of `disp-async`,
   `sync-end`, `after`).

2. **Green: bake the `with` chain into `EnterScope`/`ExitScope`.**
   - In `transform_scope_block`: capture `with_vars = ctx.with_scopes.clone()`
     *before* finalizing the `EnterScope` terminator (so if any plain
     statements are already pending in `ctx.current_statements` from earlier
     in the same `with`-body — e.g. `with (o) { L(1); { await using a = ...; } }`
     — that flush still gets the existing per-state AST wrap, unchanged).
     Finalize `EnterScope { body_state, with_vars }`. Then
     `let saved = std::mem::take(&mut ctx.with_scopes)` before transforming
     the scope's interior (`transform_statements`), and restore
     `ctx.with_scopes = saved` right after `ctx.scope_depth -= 1`, before
     control returns to the caller — so any statements textually after the
     scope block but still inside the same enclosing `with` (e.g. `with (o)
     { await using a = ...; L(2); }`, where `L(2)` is *not* inside the
     disposable-declaring block) still get the per-state wrap. This makes
     `finalize_current_state`'s existing `!self.with_scopes.is_empty()` check
     naturally skip rewrapping every state inside the scope block, with no
     change needed to `finalize_current_state` itself.
   - In `eval.rs`'s `EnterScope` handler: build the with-environment chain
     from `with_vars` (outermost first) on top of `term_env`, each via
     `Environment::new_with_object` after a `self.to_object` conversion of
     the var's current value (read via `self.env_get(&term_env, with_var)`);
     create `scope_env` as a plain child of the innermost with-environment;
     set `self.has_ever_entered_with = true` whenever `with_vars` is
     non-empty.
   - Slice 1's test goes green.

3. **Correctness beyond ticks: binding visibility and scope-chain order.**
   Two more test262-extra cases (same file or a sibling), both currently
   broken by the *old* per-state rewrap even though they don't probe ticks:
   - A binding declared by the scoped `await using` must survive a
     suspension and still be readable afterward:
     `with ({}) { await using a = r; await null; /* a still resolves */ }`.
     Under the old code, each state got a *fresh* `With(Block(...))`, so `a`
     declared in one state's throwaway block env is invisible in the next.
   - The with-object's property must correctly shadow an outer binding of
     the same name for code inside the scope, proving the chain order is
     `oldEnv -> withEnv -> scope_env` and not inverted:
     `with ({a: 'obj'}) { await using a = resource; await null; /* reads the resource, not the string */ }`
     plus the converse — a *different*-named property read through `with`
     resolves to the object's value, not an outer variable.

4. **Terminator operands resolve through the chain.** `Return`, `Throw`, and
   condition expressions in states inside the scope are evaluated directly
   against `term_env` by the driver (`eval.rs`'s per-terminator `operand!`
   calls), bypassing the AST-statement path entirely. Add a case proving this
   already works once slice 2 lands — e.g. `with ({v: 'obj'}) { await using a
   = r; return v; }` resolving `v` through the with-object. (Expected to pass
   once slice 2 is in; written as its own case because it exercises a
   different code path than slice 3's plain statement execution.)

5. **Nesting and nearby call sites.**
   - Nested `with`: `with (a) { with (b) { await using x = r; } }` — proves
     `with_vars`/the take-and-restore are captured and nested correctly (two
     with-environments chained, innermost first, not duplicated or dropped).
   - `with (null)` (or `undefined`) rejects with a `TypeError` from `ToObject`
     before the block's resource is ever created or disposed — proves the
     `pending_exception`/`continue` throw path in the new `EnterScope` arm is
     wired correctly and does not leak a resource registration.
   - The `try`-block call site (`transform_scope_block` call at line 3540):
     `with (o) { try { await using a = r; } finally { ... } }` — exercises a
     second (of the three non-plain-block) call sites through the same fix
     without any call-site-specific code.
   - A `throw` from inside the scoped block, still under `with`, disposes
     correctly and propagates the original error (not swallowed or
     replaced) — exercises `ExitScope`'s abrupt-completion interaction with
     the now-correct `scope_env`.

6. **Full regression pass.** Targeted test262 directories (below) plus the
   full `test262-extra` suite, once under the plain binary and once under
   `JSSE_GC_STRESS=1` (per `AGENTS.md`'s GC Stress Mode guidance, since
   `EnterScope`'s new environment-chain construction runs at a safepoint
   boundary), then the full test262 run to confirm the baseline doesn't
   regress.

## 5. Test surface

- `test262/test/language/statements/with/` — general `with` conformance;
  nothing here already covers suspension/disposal interaction (that's a
  jsse-internal lowering concern test262 doesn't anticipate), but the full
  directory must stay green since the fix touches shared `with` plumbing
  (`Environment::new_with_object` extraction touches `exec.rs`'s direct
  interpreter path too).
- `test262/test/language/statements/for-await-of/`,
  `test262/test/language/expressions/dynamic-import/` and anywhere else
  `await using`/`using` is exercised under `features: [explicit-resource-management]`
  — run the full `built-ins/` and `language/` suites since `await using`
  lowering is shared machinery (`uv run python scripts/run-test262.py`, no
  targeted subdirectory reliably isolates this).
- None of the above actually exercises `with` wrapping `await using` —
  that composition is exactly the gap the issue found, so the real coverage
  is the new `test262-extra/` files from slices 1, 3, 4, 5. Follow the exact
  naming/structure convention of
  `test262-extra/await-using-for-of-head-wraps-await-using-block-dispose-tick-alignment.js`
  (`esid`, `info` quoting the relevant algorithm steps, `flags: [async]` plus
  `noStrict` here since `with` is unavailable in strict mode, `features:
  [explicit-resource-management]`).
- `test262/` and `spec/` submodules are uninitialized in a fresh worktree;
  the implementation stage must run
  `git submodule update --init --depth 1 test262` (and `spec` if citing
  further clauses) before running any test262 command — matches the
  existing memory note `test262-submodule-init`.
- `cargo test --release` — covers any `#[test]`s near the touched modules
  and is the baseline Rust-level gate regardless of JS-visible behavior.
- Not applicable: `scripts/run-library-tests.sh`, `scripts/run-node-shim-selftest.sh`,
  `scripts/run-shim-fixtures.sh` — no library harness exercises `with`
  (legacy/discouraged, essentially absent from real-world bundled code) in
  combination with `await using` (a brand-new feature); no shim touches
  either.

## 6. Regression risk

- **IC fast-path gating around `with`.** `self.with_scope_depth` (reentrant
  counter, `exec.rs:1232`/`1235`) and `self.has_ever_entered_with` (sticky,
  never unset) both gate whether identifier/property resolution can use the
  cached fast path or must fall back to the always-correct with-aware slow
  path (`exec.rs:1339,1497,1798`; `eval.rs:694,815,4844,7506`;
  `bytecode/vm.rs:491`; `eval/access.rs:731`). The existing AST-rewrap
  mechanism keeps `with_scope_depth`'s nonzero window strictly bounded to one
  synchronous `exec_statement` call (a single state's body, which by
  construction contains no suspension point), so it can never straddle an
  `Await`. `EnterScope`/`ExitScope`, by contrast, bracket a *scope* that can
  span multiple states and genuinely suspend the function mid-way (that's
  the entire point of the fix). Incrementing/decrementing
  `with_scope_depth` symmetrically at `EnterScope`/`ExitScope` would leak a
  stale nonzero value into unrelated code that runs on the microtask queue
  while this function is suspended inside the scope — not a
  wrong-value bug (the slow path is always spec-correct), but worth avoiding.
  The plan instead only sets `has_ever_entered_with = true` (already a
  monotonic, suspension-safe flag by design) in the `EnterScope` handler,
  leaning on the engine's existing "any `with`, anywhere, forever" fallback
  rather than inventing new reentrant bookkeeping. Flagged explicitly so the
  implementation stage doesn't try to thread `with_scope_depth` through
  `ScopeFrame` as a "more precise" alternative — slice 3/4's tests close the
  correctness gap this would otherwise leave (an identifier reference inside
  the scope resolving wrong because neither flag was set yet).
- **GC rooting.** `gc.rs:1265` already traces `with_object` generically for
  any `Environment`, so a `scope_env` ancestor built via
  `Environment::new_with_object` needs no new GC-visibility work — confirm
  with a `JSSE_GC_STRESS=1` run over the new tests (slice 6) rather than
  auditing `gc.rs` further.
- **`Environment::new_with_object` extraction touches the direct
  tree-walker's `with`-statement path** (`exec.rs:1198`), which is used far
  more often than the async-function lowering (every non-transformed `with`
  anywhere in the engine, sync or async). A behavior-preserving extraction
  (identical field values, same call order) should be invisible to test262,
  but run the full `with`/`using` test262 directories, not just the new
  targeted files, to catch an accidental field-order or default mistake.
- **`transform_scope_block`'s other three call sites** (try-block,
  catch-clause body, finally-block — generator_transform.rs:3455, 3540,
  3601) inherit the fix automatically since they share the function; slice 5
  adds one targeted test for the try-block site, but the full
  `test262-extra` suite (which already has `await-using-block-in-finally`-style
  coverage from prior issues) is the backstop for the other two.
- **`scope_depth`/`scope_stack` bookkeeping is untouched** — only what
  `EnterScope` uses to build `scope_env`'s *parent* changes; `scope_depth`
  accounting, `reconcile_scope_stack`, and `ExitScope`'s pop/dispose logic
  are unmodified, so break/continue/return/throw unwinding through a scoped
  `with`+`await using` block should already work via the existing
  mechanism — covered incidentally by slice 5's throw case rather than
  requiring new unwinding logic.
- **`test262-pass.txt` baseline** — not rewritten by this PR (read from
  `origin/main`); if the fix flips any currently-failing upstream test262
  case to passing, that's a bonus the baseline roll-forward (a separate,
  main-branch-only operation) will pick up later, not something to chase
  here.

## 7. Out of scope

- **Async generators and sync generators.** `EnterScope`/`ExitScope` are
  emitted only for plain async function bodies (`generator_runtime.rs:2183,2185`
  and `5799,5801` both `unreachable!` on them). `scan_await_using` still
  forces `Blocked` for anything under a `with` regardless of function kind,
  but generators route a `Blocked` await-using block through the separate
  inline/replay fallback (`generator_runtime.rs`, `SentValueBindingKind::InlineYield`,
  per `AGENTS.md`'s Architecture Notes) rather than through `EnterScope`. If
  that path has an analogous bug it needs its own issue — not investigated
  here, since the reported repro and its diagnosis are specifically about
  the plain-async-function driver in `eval.rs`.
- **Eager `ToObject` timing.** Per `sec-with-statement-runtime-semantics-evaluation`,
  `ToObject` should run immediately when the `with` is entered, before its
  body executes at all. The current lowering (both before and after this
  fix) defers the conversion to whenever the with-environment is actually
  materialized — for an `EnterScope`'d block, that's still "as soon as the
  scope opens," which is as early as this fix can reasonably make it without
  a broader restructuring, but for an *ordinary* (non-disposable) `with`
  body split across states, conversion already happens per-state at the
  existing AST-rewrap, functionally unchanged by this PR. Pre-existing,
  not introduced or worsened by this fix; not addressed here.
- **A unified `ScopeFrame`-based representation for *all* `with` lowering**
  (not just the `EnterScope`/disposable case), which would also fix a
  separate pre-existing inversion for a plain `let` declared directly inside
  a `with` body that itself gets split across suspension points (its
  `OpenBlock` environment currently sits *outside* the per-state with-wrap
  rather than inside it, per the advisor review during planning). This is a
  real, related gap, but it requires touching every `with` lowering call
  site and the ordinary (non-EnterScope) `ScopeAction::OpenBlock` path, a
  materially larger change than this issue's narrow `EnterScope`/`ExitScope`
  fix — tracked as a follow-up rather than bundled in, consistent with
  "many small changes beat one large change."
- **Rewriting `test262-pass.txt`.** Main-branch-only operation, not part of
  this PR regardless of outcome.
- **Formatting/refactoring unrelated to the fix.** The
  `Environment::new_with_object` extraction is included only because the fix
  itself introduces the second call site that makes the duplication
  real — no other opportunistic cleanup in `exec.rs`/`eval.rs` is in scope.
