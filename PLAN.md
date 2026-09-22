# Plan: issue #726 — `await` in catch-parameter / for-in/of-head / for-init destructuring defaults is evaluated by the tree-walker

## 1. Problem restated

Issue #709 taught `generator_transform.rs` to lower an `await` hiding inside an object
destructuring pattern's default or computed key — but only for a plain `var`/`let`/`const`
declaration statement, where the pattern is bound by a state the transform controls end to end.
Three more binding sites exist where a pattern is bound directly by driver code that has no
state boundary of its own: the catch clause's parameter (`StateTerminator::EnterCatch { param }`,
bound via `bind_pattern` inside the `EnterCatch` match arm), a for-in/of loop's head
(`StateTerminator::ForOfHead { left }`, bound the same way in the `ForOfHead` arm), and a
C-style `for (var {a = await 1} = …;;)` initializer (whose declarator pattern the loop's own
gating condition never inspects, so the declaration is emitted intact instead of being routed
through the already-correct `transform_variable_declaration`). In all three, the tree-walker's
`bind_pattern` reaches the default via `eval_expr` → `Expression::Await` → the blocking
`await_value`, which drains the microtask queue inline instead of suspending — jobs scheduled
before the `await` run after it (`w1,c5,w2,b6,w3,sync-end` instead of
`sync-end,w1,c5,w2,b6,w3`). Worse, because the top-level suspension detector
(`generator_analysis::contains_suspension`) never looks inside a catch `param`, a for-in/of
`left`, or a C-style for-loop declarator's pattern either, a function whose *only* suspension is
in one of these three places is not recognized as needing the state machine at all, and runs
fully synchronously.

## 2. Spec basis

- `sec-runtime-semantics-catchclauseevaluation` (*CatchClauseEvaluation*): creates `catchEnv`
  (a declarative environment child of the running lexical environment), binds the catch
  parameter's names into it via `BindingInitialization`, then evaluates the catch block's
  `Statement`. The parameter's own environment is what the driver's `EnterCatch` arm already
  builds (`catch_env` in `eval.rs`); this issue is about what runs *inside* that
  `BindingInitialization` step when it reaches an `await`.
- `sec-runtime-semantics-forinofheadevaluation` (*ForIn/OfHeadEvaluation*, current id; old id
  `sec-runtime-semantics-forin-div-ofheadevaluation-tdznames-expr-iterationkind`): declares the
  lexical head's bound names in TDZ in a throwaway environment *before* the iterable expression
  is evaluated. This is `ForOfInit`/`for_of_head_tdz_env` today and **must keep reading the
  original pattern** — it only needs `BoundNames`, which lowering must not disturb.
- `sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`
  (*ForIn/OfBodyEvaluation*), lexical-binding branch: each iteration creates a **fresh**
  `iterationEnv` (`NewDeclarativeEnvironment`, not a copy-forward of the previous iteration —
  that mechanism is `sec-createperiterationenvironment`, which belongs to the C-style `for`
  loop, not `for-in`/`for-of`), performs `ForDeclarationBindingInstantiation`, sets it as the
  active lexical environment, then calls `ForDeclarationBindingInitialization` with that
  environment active. This is where an `await` in the head pattern's default needs to suspend,
  and it is exactly the environment the driver's `ForOfHead` arm already builds as `bind_env`
  and threads through `for_of_stack[loop_pos].iteration_env` /
  `ForOfLoopState::effective_env()`.
- `sec-forbodyevaluation` (C-style `for`): its own per-iteration environment is
  `sec-createperiterationenvironment`, unrelated to the two ForIn/Of clauses above; the
  C-style-for slice of this issue does not touch that machinery at all — it only needs the
  existing declaration-statement lowering to be reached.
- `sec-runtime-semantics-keyedbindinginitialization` (*KeyedBindingInitialization*): the same
  clause #709 already lowers to (`lower_pattern_binding`/`lower_pattern_property` in
  `generator_transform.rs`). This plan reuses that machinery verbatim for all three sites; it
  does not change what `KeyedBindingInitialization` lowering does, only *where* it is invoked
  from.

`ForInOfLeft::Pattern`/`ForInOfLeft::Expression` heads (bare assignment-target for-of/for-in,
e.g. `for ({a = await 1} of xs)`) are governed by
`sec-runtime-semantics-keyeddestructuringassignmentevaluation`, a different binding mechanism
(assignment, not `BindingInitialization`) that inherits #724's blocker, not this issue's — see
§7.

## 3. Files to touch

- `src/interpreter/generator_analysis.rs` — new `for_in_of_left_needs_lowering`, composed
  entirely from the existing `pattern_needs_lowering` (no new await-detection predicate):
  ```rust
  pub(crate) fn for_in_of_left_needs_lowering(left: &ForInOfLeft) -> bool {
      match left {
          ForInOfLeft::Variable(decl) => {
              decl.declarations.iter().any(|d| pattern_needs_lowering(&d.pattern))
          }
          ForInOfLeft::Pattern(_) | ForInOfLeft::Expression(_) => false,
      }
  }
  ```
  (same shape the `Statement::Variable` arm of `contains_suspension` already uses). Extend
  `contains_suspension`'s `Statement::Try`, `Statement::ForIn`, `Statement::ForOf` and
  `Statement::For` arms to consult it (catch `param`, for-in/of `left`, for-loop declarator
  patterns respectively); unit tests in its `tests` module.
- `src/interpreter/generator_transform.rs`:
  - `transform_try_statement` — strip the catch parameter to a temp identifier once, at the
    point `catch_info`/the `EnterCatch` terminator is built; emit the lowered binding as the
    first states of `catch_body_state`.
  - `transform_for_in_of_loop` — strip `ForOfHead.left` (never `ForOfInit.left`) to a temp
    identifier when the head's declarator pattern needs lowering; emit the lowered binding as
    the first states of `body_state`, after `ctx.for_of_depth += 1`.
  - `transform_for_statement` — widen the `ForInit::Variable` gating condition (currently only
    `d.init`) to also check `pattern_needs_lowering(&d.pattern)`, so `transform_variable_declaration`
    (unchanged) is reached instead of the statement being emitted intact.
  - `stmt_has_suspension`'s for-await-specific guard (`is_async && f.is_await &&
    !for_in_of_left_contains_suspension(&f.left)`) — **conditional edit, decided empirically in
    slice 4** (see §4): only touched if slice 4's test fails without it.
  - Unit tests in its `tests` module for all three sites.
- `docs/adr/2026-09-21-2143-destructuring-pattern-lowering.md` — update the "What this change
  does not cover" list: remove the catch/for-heads and `for (…;;)`-initializer bullets (now
  covered), note the `ForOfInit`-vs-`ForOfHead` `left` split as the invariant that made it safe.
- `CONTEXT.md` — extend the existing **Pattern Lowering** glossary entry: it currently says
  "Array patterns, catch parameters, for-in/of heads and assignment forms are not lowered
  yet" — drop "catch parameters" and "for-in/of heads" from that sentence and add one line on
  the strip-to-temp shape (pattern replaced by a temp identifier in the terminator that binds
  it; the lowered binding runs as the first states of the state the driver already opened).
- `test262-extra/*.js` — new regression tests (§5).

## 4. TDD slices

Each slice is red (new test fails on today's binary) → green (production change makes it pass).
Build with `cargo build --release -j4` (cap parallelism); run new tests with `timeout`. Quality
gates as **separate** commands per CLAUDE.md: `cargo fmt --check`, `cargo clippy -- -D
warnings`, `cargo test --release`, `./scripts/lint.sh`.

1. **C-style `for` initializer — the cheapest slice, no driver change.**
   Test: `test262-extra/async-function-for-init-destructuring-default-await-order.js` —
   `for (var {a = await 1} = {};;)` (and `let`, `const`) inside an async function, asserting
   `sync-end` precedes the tick the default's `await` resolves on, mirroring the issue's own
   ordering assertion. Also assert the *no-other-await* case: this loop is the *only*
   suspension in the function, to exercise the `contains_suspension` top-level gate, not just
   the inner lowering.
   Code: `contains_suspension`'s `Statement::For` arm gains
   `|| pattern_needs_lowering(&d.pattern)` per declarator (generator_analysis.rs);
   `transform_for_statement`'s `ForInit::Variable` gate gains the same disjunct
   (generator_transform.rs). `transform_variable_declaration`/`lower_pattern_binding` are
   untouched — this slice only has to get them invoked.

2. **Catch parameter.**
   Tests:
   - `test262-extra/async-function-catch-destructuring-default-await-order.js` — the issue's
     own `catch ({a = await 5})` repro in isolation (no for-of), asserting `sync-end` precedes
     the resumed tick.
   - `test262-extra/async-function-catch-destructuring-default-await-scope.js` — the catch
     parameter's bound names are visible inside the catch body and are *not* visible after the
     `try`/`catch` statement (leak check into function scope), with a suspending default in the
     mix so the lowered path is exercised, not just the pre-#726 one.
   - A rejecting default (`catch ({a = await Promise.reject(new Error("x"))})`) propagates as
     an uncaught rejection rather than being silently swallowed (matches `?` propagation in
     `BindingInitialization`).
   Code: in `transform_try_statement`, where `catch_info`/`EnterCatch` is built (~2645-2708):
   compute `pattern_needs_lowering(&info.param)` once; if true, allocate a temp
   (`ctx.new_temp_var("catch_param")`), finalize `EnterCatch` with
   `param: Some(Pattern::Identifier(temp))` instead of `info.param.clone()`, then — after
   `ctx.current_state_id = catch_body_state; ctx.scope_depth += 1;` and before
   `transform_clause_body` — call `lower_pattern_binding(VarKind::Let, &original_param,
   &temp, ctx)`. `catch_body_state` keeps its existing no-`OpenBlock` scope action, so
   `reconcile_scope_stack` resolves to the driver-pushed `catch_env` for both the temp bind and
   the lowered names — this is the invariant the scope test checks.
   Leave `TryEnter`'s `CatchInfo.param` (used only for `clear_terminator_ic_sites`, never read
   by the driver) as the original pattern — do not thread the temp through it.

3. **For-in/of head — `var`/`let`/`const`, plain (non-`await`) loop, the issue's own repro.**
   Tests:
   - `test262-extra/async-function-forof-destructuring-default-await-order.js` — the issue's
     `for (var {b = await 6} of [{}])` in isolation, asserting `sync-end` precedes the resumed
     tick, for `var`, `let`, `const`.
   - `test262-extra/async-function-forin-destructuring-default-await-order.js` — same shape for
     `for-in` (`for (var {a = await 1} in {x: 1})`), since `for-in` shares `ForOfHead` in this
     engine.
   - `test262-extra/async-function-forof-destructuring-default-await-per-iteration-env.js` —
     `for (let {a = await 1} of [{}, {}])` pushing a closure per iteration into an array; after
     the loop, each closure returns its *own* iteration's `a`, proving the lowered binding
     landed in the fresh per-iteration environment and not a shared one.
   - `test262-extra/async-function-forof-destructuring-default-await-unwind.js` — `break` and
     `return` from inside the loop body on the iteration *after* the head's default has already
     suspended once (so unwinding crosses a state the lowering introduced), and a default that
     rejects (iterator's `return()` must still be called exactly once — instrument the source
     iterable's `return` method and assert the call count).
   - `test262-extra/async-function-forof-destructuring-default-await-tdz.js` — the
     TDZ-preservation invariant, promoted from the risk analysis (§6) because it is the one
     test that actually distinguishes "stripped `ForOfHead.left` only" from "stripped both":
     `for (let {a = await 1} of [a]) {}` must still throw a `ReferenceError` (the iterable
     expression references `a` while it is in TDZ). Run it both with this being the function's
     only suspension and alongside an unrelated `await` elsewhere, since `ForOfInit`'s TDZ
     environment is built from `for_of_head_tdz_env`'s `BoundNames` walk over the *original*
     `left`, independent of whether lowering fires at all.
   Code: in `transform_for_in_of_loop`, at `ForOfHead` construction (~2579-2587): for
   `ForInOfLeft::Variable(decl)` whose sole declarator's pattern needs lowering, allocate a
   temp (`ctx.new_temp_var("forof_head")`), finalize `ForOfHead` with `left` rewritten to
   `ForInOfLeft::Variable(VariableDeclaration { kind: decl.kind, declarations: vec![
   VariableDeclarator { pattern: Pattern::Identifier(temp), init: None } ] })`. **Do not touch
   `ForOfInit.left`** — it stays `left.clone()` as today, so `for_of_head_tdz_env`'s
   `BoundNames` walk still sees the real names for the TDZ environment. After
   `ctx.current_state_id = body_state; ctx.for_of_depth += 1;` and before the existing
   `stmt_has_suspension(body, …)` branch, call `lower_pattern_binding(decl.kind,
   &original_pattern, &temp, ctx)`. `for_of_head_lexical`/`needs_iter_env` in the driver key off
   `decl.kind`, not the pattern, so the fresh per-iteration environment
   (`ForOfLoopState::iteration_env` / `effective_env()`) is still created exactly as today, and
   `term_env` for the new lowering states resolves to it via the existing `reconcile_scope_stack`
   / `for_of_env` machinery — no driver change.
   For the `var` variant of this slice's headline repro, `bind_pattern`'s `Pattern::Identifier`
   arm (`exec.rs:1368`) takes a different path than `let`/`const`: with `BindingKind::Var` it
   calls `Environment::find_var_scope(env)` (walking up from `bind_env`, which for `var` is
   `outer_env`, not a fresh per-iteration env) to find `$tmp`'s already-declared slot — put
   there by the function-entry `temp_vars` loop (`eval.rs:8124`) — and assigns into that slot
   via `env_set` rather than creating a new binding local to `bind_env`. The lowering states
   that follow (which read `Identifier($tmp)` by ordinary chain lookup from the same `bind_env`)
   see the same func-scoped slot, so this is consistent: it is exactly how an unlowered
   `for (var b of …)`'s single, reused, function-scoped `b` already behaves today, just
   parameterized by a synthetic name instead of the user's.

4. **For-await-of head, plain object-pattern default (only the lowerable shape).**
   Test: `test262-extra/async-function-for-await-of-destructuring-default-await-order.js` —
   `for await (let {a = await 1} of asyncIterableOf([{}]))` as the function's *only*
   suspension (no other `await` anywhere), asserting correct interleaving and that the
   mandatory per-step `Await(nextResult)` still happens once per iteration (instrument the
   async iterable to count `next()` calls).
   Code: slice 3's `transform_for_in_of_loop` edit is already `is_await`-agnostic (same
   function, same `ForOfHead` terminator, for both `for-of` and `for-await-of`), so **write and
   run this test first, against slice 3's code with `stmt_has_suspension`'s existing for-await
   guard (`is_async && f.is_await && !for_in_of_left_contains_suspension(&f.left)`) left
   untouched.** Trace it: the guard's `!for_in_of_left_contains_suspension(&f.left)` is false
   here (the head does contain a suspension), so `stmt_contains_for_of_head` does not return
   early via that branch, but execution falls through to `contains_suspension(stmt)`, which now
   returns `true` via slice 1-3's new `Statement::ForOf` arm check
   (`for_in_of_left_needs_lowering`) regardless. If the test passes with no code change, **make
   no edit** — leave the existing guard as is, and instead add one line to §7 noting that a
   for-await-of head with a *non-lowerable* pattern (array pattern, object rest) is a latent,
   pre-existing gap this plan does not touch, tracked under #725. Only if the test fails, swap
   the guard's predicate to `for_in_of_left_needs_lowering` so it agrees with the
   `contains_suspension` arm, and re-add the file-list entry removed from §3.

## 5. Test surface

No test262 coverage exists for this behavior today — verified by initializing the submodule and
grepping: zero files under `test262/test/language/statements/try/dstr/`,
`test262/test/language/statements/for-of/dstr/`, or `test262/test/language/statements/for-in/dstr/`
reference `await` (these are generic dstr fixtures shared across binding contexts, not
async-specific), and `test262/test/language/statements/for-await-of/`'s `await`-containing files
are array-pattern (`dstr-array-elem-*`) fixtures reusing `async-function` test names, not the
object-pattern-default-ordering shape this issue is about. All four TDD slices' tests therefore
belong in `test262-extra/`, following `test262-extra/async-function-destructuring-default-await-suspends.js`'s
harness pattern from #709 (a shared `log`/timestamp array, `Promise.resolve().then()` chain
markers, `setTimeout(() => print(log.join(",")), 0)` at the end) rather than inventing a new one.

Targeted regression runs (must not move `test262-pass.txt`, read from `origin/main`, so no
`--update-baseline` in this branch):
- `uv run python scripts/run-test262.py test262/test/language/statements/try/`
- `uv run python scripts/run-test262.py test262/test/language/statements/for-of/`
- `uv run python scripts/run-test262.py test262/test/language/statements/for-in/`
- `uv run python scripts/run-test262.py test262/test/language/statements/for-await-of/`
- `uv run python scripts/run-test262.py test262/test/language/statements/for/`
- `uv run python scripts/run-test262.py test262/test/language/statements/async-function/`
  and `test262/test/language/statements/async-generator/` (the `stmt_has_suspension` guard
  edit and `contains_suspension` arm edits are shared machinery)
- `uv run python scripts/run-custom-tests.py` (test262-extra + tests/)
- Full suite: `uv run python scripts/run-test262.py`, per CLAUDE.md, after the targeted runs are
  clean.

## 6. Regression risk

- **Shared machinery.** All four edits sit in `generator_analysis::contains_suspension` and
  `generator_transform::stmt_has_suspension`/`transform_try_statement`/
  `transform_for_in_of_loop`/`transform_for_statement` — functions every `try`, every
  `for`/`for-in`/`for-of`/`for-await-of`, and every async function/async generator body passes
  through during state-machine construction. A mistake here is not contained to destructuring;
  it can misclassify any try/loop statement's suspension status. The targeted test262 directories
  in §5 exist specifically to catch that before the full suite runs.
- **`ForOfInit.left` vs `ForOfHead.left` divergence is the single highest-risk detail.** Both
  terminators independently clone `left` today; stripping the wrong one (or both) silently
  breaks TDZ, but only observably so for a head that actually lowers — `for (let {a} of [a])`
  has no `await`, so `pattern_needs_lowering` is false and nothing gets stripped regardless of
  whether this bug exists, making it a useless regression test. The discriminating case is
  `for (let {a = await 1} of [a]) {}` (slice 3's TDZ test, §4): its `left` *does* get replaced
  in `ForOfHead`, so this is the one case where a "strip both" mistake would actually surface —
  `a` would resolve outward through the TDZ environment instead of throwing.
- **`stmt_has_suspension`'s for-await guard (slice 4)** is the one existing line this plan
  repurposes rather than only extends. Getting its replacement predicate wrong in the "too
  broad" direction is low-risk (redundant true, `for_await` already always suspends); "too
  narrow" is higher-risk (a for-await loop stops being recognized as suspending at all,
  regressing before #726 even touched it) — slice 4's test must run standalone (no other
  suspension in the function) to catch that direction specifically.
- **GC rooting / `gc_safepoint()`**: the new temp lives in `catch_env`/the for-of
  `iteration_env`, both already GC roots via `scope_stack`/`for_of_stack` (unchanged structures,
  only their contents' pattern differs) — no new rooting path.
- **Bytecode fast path**: `bytecode_enabled` is off by default and async-function
  state-machine bodies are a tree-walker/state-machine concern only; `bytecode/` is not touched.
- **`ObjectKind` exhaustive matches**: unaffected — no new object kind.
- **Library harnesses**: none of the pinned libraries (`decimal.js`, `big.js`, `acorn`, `zod`,
  `moment`, etc.) exercise `await` inside a catch/for-head/for-init destructuring default in
  their own source, based on the shapes each library's shim wiring targets; not re-run as part
  of this fix, per existing practice (they're not gated on interpreter-internal edge cases like
  this one).

## 7. Out of scope

- **`ForInOfLeft::Pattern`/`ForInOfLeft::Expression` for-of/for-in heads** (bare assignment
  targets: `for ({a = await 1} of xs)`, `for (o.x = await 1 of xs)` is invalid syntax but
  `for ([a = await 1] of xs)`-shaped assignment patterns apply) — these bind via
  `KeyedDestructuringAssignmentEvaluation`/`assign_to_for_pattern`, not `BindingInitialization`,
  and inherit #724's blocker (`extract_lhs_suspensions` only handles `Member`, so the LHS
  `Expression` already rewritten to `Yield` still hangs the driver). Fixing that is #724's job,
  not this one's — bundling it here would tie two independently-landable fixes together.
- **Array patterns in any of the three sites** (`catch ([a = await 1])`,
  `for (var [a = await 1] of xs)`, `for (var [a = await 1] = [];;)`) — `lower_pattern_binding`
  does not support array patterns at all yet (#725: the iterator record must be held across
  suspension and closed exactly once on abrupt exit, which needs new interpreter-internal
  helpers, not just a transform change). `pattern_lowering_supported`/
  `for_in_of_left_needs_lowering` already exclude these, so this plan changes nothing about
  their (unchanged, still-buggy) behavior — no regression, no fix.
- **An object rest beside a suspending sibling** (`catch ({a = await 1, ...rest})`) — same
  #725 gap as #709 documented for the plain declaration case; `pattern_lowering_supported`
  already returns `false` for `ObjectPatternProperty::Rest`, unchanged by this plan.
- **`yield` in a catch/for-head/for-init declaration pattern**, sync or async generators — a
  separate, already-tracked gap (per the ADR, "#727 `yield` in a declaration pattern"); this
  plan's predicates (`pattern_needs_lowering`, `for_in_of_left_needs_lowering`) are
  await-triggered only, by design, matching #709's own trigger.
- **`using`/`await using` for-of heads** — the grammar restricts these to a single
  `BindingIdentifier` (no destructuring pattern is syntactically possible), so there is nothing
  for `pattern_needs_lowering` to ever match there; not touched, not tested beyond existing
  coverage.
- **No refactor of `bind_pattern`'s residual `_ => JsValue::UNDEFINED` swallow arms** (three
  sites in `exec.rs`) — still reachable from the sync-generator `yield`-in-pattern path this
  plan does not touch; adding a `debug_assert!` there would fire on live inputs, per #709's own
  note.
