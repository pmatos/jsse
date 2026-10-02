# Plan: issue #776 — block-scoped `let`/`const` whose own initializer suspends is a func-level temp var, not TDZ'd

## 1. Problem restated

In `transform_generator`/`transform_async_function`'s `transform_variable_declaration`
(`src/interpreter/generator_transform.rs`), a `let`/`const` declarator with a **plain
identifier** pattern whose own initializer contains the suspension point (`const x = yield`,
`const x = await p`) is special-cased: the name is pushed into the function-level `temp_vars`
list and the suspended value is written straight into that `func_env` slot via
`SentValueBindingKind::Variable(name)`. This bypasses the normal `let`/`const` binding
machinery entirely. Two consequences, both reproduced in the issue against `d1b12137` (pre-#738)
and still present today since #738 deliberately left this shape unfixed:

1. **No TDZ.** Because the name never enters the block's own `Environment` via
   `collect_block_lexical_decls`/`ScopeAction::OpenBlock`, a closure created earlier in the same
   block that reads the name observes no TDZ at all — it simply doesn't exist as a binding in the
   block's `Environment`, so lookup falls through to whatever the chain finds next.
2. **Identity collision.** If an outer `var` of the same name exists in the same function, the
   "temp var" *is* that `var`'s own `func_env` slot (same name, same environment) — resuming the
   generator overwrites the outer `var`, instead of writing into a separate per-block binding that
   gets discarded when the block exits.

The fix (per the issue's own analysis) is to stop special-casing the identifier shape and route it
through the same fresh-temp + `emit_pattern_binding` mechanism the non-identifier (destructuring)
branch already uses correctly — the one `#738` relied on for every *other* lexical shape. Once
every declarator takes that path, `declarator_enters_block_env`'s whole reason for existing (the
doc comment's shadowing concern) stops applying, and both its call sites (`collect_block_lexical_decls`,
and the `for`-loop head's `initial_lexical_bindings`) can drop their filter.

## 2. Spec basis

- **`sec-block`** (Block, Runtime Semantics: Evaluation) — entering a `Block` creates a fresh
  Declarative Environment Record (`NewDeclarativeEnvironment`) and runs
  `BlockDeclarationInstantiation` against it before any statement of the block executes.
- **`sec-blockdeclarationinstantiation`** (BlockDeclarationInstantiation) — *every* lexically
  scoped declaration of the block (its `LexicallyScopedDeclarations`, a static, whole-block scan)
  gets an uninitialized binding (`CreateImmutableBinding`/`CreateMutableBinding`) in that
  Environment Record up front, independent of where inside the block a suspension later splits
  execution. This is exactly what `collect_block_lexical_decls` + `ScopeAction::OpenBlock` model,
  and exactly what `declarator_enters_block_env` currently defeats for one declarator shape.
- **`sec-let-and-const-declarations-runtime-semantics-evaluation`**, production
  `LexicalBinding : BindingIdentifier Initializer` — evaluation is `ResolveBinding` (finds the
  *already-declared*, uninitialized binding from `BlockDeclarationInstantiation`) followed by
  `InitializeReferencedBinding` once the initializer's value is known. It does **not** create the
  binding itself. This is the behavior `emit_pattern_binding`'s synthesized `Statement::Variable`
  already produces for every other shape (via `bind_pattern`'s `kind != Var` arm,
  `exec.rs:1461-1464`) and the one the identifier shape needs to join.
- **`sec-declarative-environment-records-getbindingvalue-n-s`** (GetBindingValue) — "If the
  binding exists but is uninitialized a *ReferenceError* is thrown" — this is the TDZ enforcement
  that never fires for the current identifier shape (repro #1), because no binding for the name
  exists in the block's Environment Record at all until the temp-var assignment lands in `func_env`.
- **`sec-for-statement`** (ForStatement Evaluation, `LexicalDeclaration` production) +
  **`sec-createperiterationenvironment`** — a `for (let/const ...; ...; ...)` head creates its own
  Declarative Environment and pre-creates (im)mutable bindings for the head's `BoundNames` before
  evaluating the `LexicalDeclaration`, same shape as `BlockDeclarationInstantiation`. This governs
  the `initial_lexical_bindings` pre-declare at `generator_transform.rs:2899-2915`, whose
  `.filter(declarator_enters_block_env)` has the identical bug for `for (let i = yield 1; ...)`.

No new syntax or semantics are introduced; this closes a gap between the engine's generator/async
lowering and the clauses above, which the non-identifier declarator shape already satisfies.

## 3. Files to touch

- `src/interpreter/generator_transform.rs` — the only production file:
  - `transform_variable_declaration` (~line 2561-2585): collapse the `match &declarator.pattern { Pattern::Identifier(name) => ..., pattern => ... }` into the single generic path (fresh temp + `emit_pattern_binding(decl.kind, declarator.pattern.clone(), &source, ctx)`), for every pattern shape.
  - Delete `declarator_enters_block_env` (~line 971-988) and its doc comment.
  - `collect_block_lexical_decls` (~line 990-1025): drop the `if !declarator_enters_block_env(d, is_async) { continue; }` filter and the now-unused `is_async: bool` parameter; update its doc comment (the "Declarators that `declarator_enters_block_env` excludes are left out entirely" sentence goes away — every lexical name is now included, matching `sec-static-semantics-lexicallyscopeddeclarations` without exception).
  - 4 call sites losing the now-removed `is_async`/`ctx.is_async` argument: `collect_block_lexical_decls(stmts, ...)` (~1088), `collect_block_lexical_decls(&try_stmt.block, ...)` (~3310), `collect_block_lexical_decls(&h.body, ...)` (~3329), `collect_block_lexical_decls(finalizer, ...)` (~3371).
  - The `for`-loop head's `initial_lexical_bindings` (~line 2891-2915): drop
    `.filter(|d| declarator_enters_block_env(d, ctx.is_async))` and the doc comment explaining why
    it was needed (replace with a short note that every bound name of the head now enters the
    per-iteration Environment, matching every other lexical shape).
- `test262-extra/` — new regression files (see §5).
- No `docs/adr/` entry: this is a bug fix restoring spec-mandated behavior that #738 already
  established the pattern for, not a new architectural decision. No `CONTEXT.md` change: no new
  vocabulary.

## 4. TDD slices

The production change is a single compilation unit: `transform_variable_declaration`'s identifier
branch and the two `declarator_enters_block_env` call sites must change together, because
`collect_block_lexical_decls`/the `for`-head filter and the identifier branch are two halves of one
mechanism — pre-declaring a name as TDZ in a block Environment that `emit_pattern_binding`'s
synthesized statement then resolves into. Fixing one without the other either fails to compile
(the fn being deleted is still referenced) or actively regresses (e.g., pre-declaring TDZ without
also rerouting the resume-time write still leaves the temp-var collision). So: one red step with
every regression test added up front, one green step with the full fix landing at once.

1. **Baseline (red, no code change).** Build release
   (`cargo build --release -j<N>`, capped per the memory budget) and run the issue's two repro
   snippets verbatim through `jsse -e`. Record the actual (buggy) output for both — confirms the
   starting point described in the issue still reproduces on this branch before any test is added.

2. **Write the regression tests (red).** Add, under `test262-extra/` (see §5 for exact files and
   content shape, following the `generator-nested-block-let-const-tdz-across-yield.js` pattern from
   #738):
   - `generator-nested-block-own-initializer-yield-tdz.js` — both of the issue's repros (TDZ-not-
     enforced, and outer-`var` identity collision) plus a positive case (the block's own `const`
     correctly observes the sent value after resume, once the first two assertions are satisfied).
   - `generator-for-head-own-initializer-yield-tdz.js` — the same outer-`var`-identity-collision
     shape, but for a `for (const i = yield; ...; ...)` head (exercises the
     `initial_lexical_bindings` filter removal specifically, a distinct call site from the block
     case).
   - `async-function-nested-block-own-initializer-await-tdz.js` — the block repro with `await`
     instead of `yield`, following `async-function-nested-block-let-const-tdz-across-await.js`'s
     `flags: [async]` / `$DONE` harness shape.
   - `async-generator-nested-block-own-initializer-yield-tdz.js` — same shape under an async
     generator (mixed `yield`/`await` suspension), following
     `async-generator-nested-block-let-const-tdz-across-yield.js`.

   Run all four against the release binary from step 1 via
   `uv run python scripts/run-test262.py test262-extra/<file>.js` and confirm every assertion that
   depends on the fix fails (TDZ not thrown / outer var clobbered), matching step 1's findings.

3. **Implement the fix (green).** Make the single coherent change described in §3: collapse the
   identifier branch, delete `declarator_enters_block_env`, simplify `collect_block_lexical_decls`
   (drop the parameter), drop the `for`-head filter, fix the 4 call sites. Rebuild release. Re-run
   the four files from step 2 and confirm every assertion now passes.

4. **Document the incidental correctness fix (green, same build).** Add
   `generator-top-level-own-initializer-yield-const-reassign-throws.js`: a **top-level** (not
   nested in a block) `const x = yield;` directly in a generator body, followed by resuming and
   then attempting `x = 2` from inside the generator. Before the fix, `x` was a mutable `func_env`
   temp var (silent success); after, `emit_pattern_binding` runs `bind_pattern`'s `kind != Var` arm,
   which calls `env.declare(name, BindingKind::Const)` over the pre-existing `BindingKind::Var`
   slot that `eval.rs`'s scope_depth-0 setup loop (`eval.rs:8022-8035`) declared at call time —
   so the name becomes a real immutable binding and the reassignment now throws `TypeError`. This
   is a `sec-let-and-const-declarations`-mandated side effect of the same fix, not a new feature;
   call this out explicitly in the PR description so a reviewer doesn't mistake it for scope creep.
   No production change in this step — it should already be green from step 3.

5. **Full regression gate.** `cargo test --release`, `./scripts/lint.sh`,
   `uv run python scripts/run-test262.py test262-extra/`, then the targeted test262 directories and
   the full suite from §5/§6 below.

## 5. Test surface

**New `test262-extra/` files** (this is squarely the "spec-correct, not covered by test262" case —
test262 has no test combining TDZ with a specific generator/async state-machine lowering choice,
since that's an implementation detail, not observable language behavior independent of engine
architecture):
- `generator-nested-block-own-initializer-yield-tdz.js`
- `generator-for-head-own-initializer-yield-tdz.js`
- `async-function-nested-block-own-initializer-await-tdz.js`
- `async-generator-nested-block-own-initializer-yield-tdz.js`
- `generator-top-level-own-initializer-yield-const-reassign-throws.js`

Run them targeted: `uv run python scripts/run-test262.py test262-extra/<file>.js`, then the whole
directory: `uv run python scripts/run-test262.py test262-extra/`.

**Targeted test262 directories** (generic TDZ/let/const/generator/async semantics the change must
not regress, even though none of them hit this exact lowering-internal bug by construction):
- `test262/test/language/statements/let/`
- `test262/test/language/statements/const/`
- `test262/test/language/statements/for/`
- `test262/test/language/statements/generators/`
- `test262/test/language/statements/async-function/`
- `test262/test/language/statements/async-generator/`
- `test262/test/language/expressions/yield/`
- `test262/test/language/expressions/await/`
- `test262/test/language/expressions/async-generator/`

**Full gates per `CLAUDE.md`:** `cargo test --release` (unit tests — none currently reference
`declarator_enters_block_env`/`collect_block_lexical_decls` by name, so none need rewriting, but
the whole suite must still pass), `./scripts/lint.sh`, and the full
`uv run python scripts/run-test262.py` run (language/, built-ins/, annexB/, intl402/) to catch any
unexpected baseline movement before the implementation stage reports results.

## 6. Regression risk

- **Shared machinery leaned on:** `ScopeAction::OpenBlock` dispatch (`exec.rs` around line 2044,
  `BlockDeclarationInstantiation`), `bind_pattern`'s `kind != Var` arm (`exec.rs:1461-1464`,
  declare-then-initialize), and `SentValueBindingKind::Variable`'s direct `func_env` write
  (`generator_runtime.rs:3005-3018` and its sibling call sites). None of these are modified — the
  fix only changes which *names* the existing mechanisms are asked to track, not how the mechanisms
  themselves work. Low risk of touching the tree-walker hot paths, the property MOP, GC rooting, or
  the bytecode fast path at all (generators always run through the tree-walking state machine, not
  the bytecode VM).
- **Every call site of `collect_block_lexical_decls`** (plain blocks, `try` body, `catch` body,
  `finally` body) now includes a strictly larger set of names in `OpenBlock`'s TDZ pre-declare.
  Risk: if some *other* code path still assumes a same-named `func_env` temp var exists for an
  identifier declarator with a suspending own-initializer (e.g., some later `SentValueBindingKind`
  dispatch keyed on the declared name rather than the fresh temp), it would now read a stale/wrong
  value. Mitigated by grep: the only consumer of `SentValueBindingKind::Variable` is the resume-time
  write in `generator_runtime.rs`, which the fix already redirects to the fresh temp consistently
  with the non-identifier branch — same call site, same pattern, no bifurcation left.
- **`using`/`await using`** identifier declarators with a suspending own-initializer go through this
  same branch today (reused-name temp var, `add_disposable_resource` presumably never sees a real
  binding to register). After the fix they flow through `emit_pattern_binding` →
  `exec_variable_declaration`'s `is_using` branch → `add_disposable_resource`, which is a *more*
  correct outcome (the resource is actually tracked for disposal) but is a behavior change worth
  flagging in the PR description; no `test262-extra` coverage is proposed for this in slice 1-4,
  since it's not in the issue's repro set — note it as a possible fast follow if time allows, not a
  blocking requirement.
- **`test262-pass.txt` baseline movement:** expected direction is tests flipping from fail → pass
  (closing the gap), not pass → fail, since the change makes behavior spec-compliant where it
  previously wasn't. Per repo convention the baseline itself is not rewritten by this PR (that's a
  `main`-branch, `--update-baseline` operation) — the implementation stage only needs to confirm no
  previously-passing test regresses.
- **Top-level (scope_depth 0) generator/async lexical TDZ is a separate, pre-existing gap, not
  touched by this fix and not fixed by it:** `eval.rs:8019-8035` declares a generator/async
  function's *own* top-level `let`/`const` names as `BindingKind::Var` (not `Let`/`Const`) before
  the state machine starts, so a top-level `let x` never gets TDZ protection regardless of whether
  its initializer suspends. This plan does not change that loop and does not claim to fix it —
  slice 4's test exercises the *const-immutability* side effect of this fix at the top level, which
  is unrelated to (and unaffected by) that separate TDZ gap. Worth a follow-up issue, not scope here.

## 7. Out of scope

- Fixing the top-level (non-block) generator/async TDZ gap described above (`eval.rs:8019-8035`
  declaring top-level lexical names as `Var`) — a distinct, pre-existing bug; candidate for a
  separate issue.
- `using`/`await using` disposal-tracking test coverage for this same code path (noted as a
  possible fast-follow in §6, not required to close #776).
- Merging `transform_variable_declaration`'s two top-level branches (the `pattern_needs_lowering`
  branch and the one this fix changes) into one — a structural refactor unrelated to the bug.
- `switch`/`CaseBlock` lexical TDZ-across-suspension (`collect_block_lexical_decls` is never called
  for `CaseBlock` at all today) — a separate, pre-existing gap this issue doesn't mention and this
  fix doesn't touch.
- `NamedEvaluation` for a class expression initializer on this branch (e.g.
  `const C = class { [yield]() {} }`) — already not handled by the existing non-identifier branch
  either; not newly broken or newly fixed here.
- The possibly-unreachable `needs_init` branch in `generator_runtime.rs`'s
  `apply_sent_value_binding` (~line 3006) — unrelated dead-code-shaped question, not touched.
- Renaming/removing the now-unused `_after_state` parameter of `transform_variable_declaration` —
  pre-existing, unrelated to this bug.
- Rewriting the baseline (`test262-pass.txt --update-baseline`) — a `main`-branch operation per
  `CLAUDE.md`, not performed from this branch.
