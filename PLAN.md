# Plan: issue #841 — switch `CaseBlock` has no per-entry lexical scope once lowered

## 1. Problem restated

`transform_switch_statement` (`src/interpreter/generator_transform.rs`) lowers a `switch`
statement inside any async function or generator (every such function is compiled through
`generator_transform.rs`, regardless of whether a given `switch` itself contains a suspension)
into a `SwitchDispatch`/`ConditionalGoto` dispatch followed by each case's statements emitted
straight into the state graph, with no scope bookkeeping at all. Per spec, a `switch`'s
`CaseBlock` is a *single* lexical environment spanning every case — `BlockDeclarationInstantiation`
runs once, before any case body executes, not per case. Because the lowering never opens (or
closes) that environment, a `let`/`const`/class declared in one case leaks into the enclosing
function scope once the switch is lowered: `case 1: let y = 'inner'; await 0; ...` clobbers an
outer `var y` for the rest of the function, where Node (and jsse's own tree-walker, for a
non-lowered switch) correctly restores the outer binding once the switch exits. `Block` and
`try`/`catch`/`finally` clause bodies already get this per-entry treatment (`ScopeAction::OpenBlock`,
issue #703); `switch` was missed. This also forces `generator_analysis.rs::scan_switch_body` to
keep a lexical sibling next to an isolatable `await using` block blocked for `switch` specifically
(unlike `Block`/`try`, where #703's scope already makes that sibling safe), since isolating the
`await using` block today would let a case-level lexical declaration flatten into the now-provably-
broken switch scope.

## 2. Spec basis

- `sec-switch-statement-runtime-semantics-evaluation` (`SwitchStatement : switch ( Expression ) CaseBlock`):
  1. Evaluate `Expression` (the discriminant) and `GetValue` it — **in the current
     (outer) `LexicalEnvironment`**, before any new environment exists.
  2. `oldEnv` = current `LexicalEnvironment`; `blockEnv` = `NewDeclarativeEnvironment(oldEnv)`.
  3. `BlockDeclarationInstantiation(CaseBlock, blockEnv)` — once, for the whole `CaseBlock`.
  4. Set the running execution context's `LexicalEnvironment` to `blockEnv`.
  5. `R` = `CaseBlockEvaluation(CaseBlock, switchValue)`.
  6. Restore `LexicalEnvironment` to `oldEnv` — unconditionally, "no matter how control leaves
     the `SwitchStatement`" (the spec's own note).
- `sec-runtime-semantics-caseblockevaluation` / `sec-runtime-semantics-caseclauseisselected`:
  case-test matching (`CaseClauseIsSelected`, the `===` comparison against each `case` expression)
  and case-body evaluation both happen *after* `blockEnv` is installed as the running
  `LexicalEnvironment` (step 4 above happens before step 5, which is where matching happens) — so
  case test expressions, not just case bodies, run inside the new scope.
- `sec-blockdeclarationinstantiation`: every lexically-scoped declaration of `CaseBlock` (every
  case's `let`/`const`/`using`/`await using`/class/function, from `LexicallyScopedDeclarations`)
  enters the one `blockEnv` together, in TDZ, before any case's statements run — this is what
  makes the scope single and shared across all cases rather than per-case.
- `sec-block-runtime-semantics-evaluation`: the already-correctly-implemented sibling clause for
  plain `Block`, cited here only as the precedent `#703`'s `ScopeAction::OpenBlock` follows —
  confirms the fix is "give `switch` the same treatment", not a new mechanism.

## 3. Files to touch

- `src/interpreter/generator_transform.rs`
  - `collect_block_lexical_decls`: generalize its parameter from `&[Statement]` to
    `impl Iterator<Item = &Statement>` so the same TDZ-name-collection logic can run over a
    flattened `switch_stmt.cases.iter().flat_map(|c| c.consequent.iter())` as well as a plain
    block's `&[Statement]`. Existing call sites (`try_stmt.block`, `finalizer`, the `Block`
    case) add `.iter()`.
  - `transform_switch_statement`: insert one bridging state (mirroring the `Block` case's
    `entry_state` bridge in `transform_yielding_statement`) right after the discriminant is
    evaluated/captured and before case-test matching begins, in **both** dispatch branches:
    - the non-suspending-tests branch (currently builds `temp_discriminant` then emits
      `StateTerminator::SwitchDispatch` directly) — the bridge must carry the `SwitchDispatch`
      terminator itself, since its case-test comparisons run inside `blockEnv` too. **This branch
      must also stop special-casing a non-suspending discriminant as a raw cloned expression**
      (`temp_discriminant = switch_stmt.discriminant.clone()`): both `StateTerminator::SwitchDispatch`'s
      `discriminant` and `cases[].test` are evaluated by a single `term_env` lookup at runtime
      (`eval.rs:9242`/`9248`, `operand!(discriminant, &term_env)`/`operand!(&case.test, &term_env, ...)`)
      — whichever environment `reconcile_scope_stack` returns for *that one state*. Since the
      bridge must live on this same state (so the case tests run inside `blockEnv`, per spec), a
      raw (non-suspending) discriminant left un-bound would now evaluate inside `blockEnv` too,
      which is wrong per `sec-switch-statement-runtime-semantics-evaluation` step 1 (discriminant
      evaluates in `oldEnv`, before `blockEnv` exists) — observable as a spurious TDZ throw for
      `let x = 1; switch (x) { case 1: let x = 2; ... }` (discriminant `x` would resolve to the
      case's own TDZ `x`, not the outer one). Fix: always route the discriminant through
      `emit_expression_with_binding`/a temp var binding in this branch too (unconditionally, not
      only `if expr_has_suspension(...)`), exactly mirroring
      `lower_switch_dispatch_with_suspending_tests` (`generator_transform.rs:3648-3654`), so the
      discriminant is always captured into a temp *before* the bridge, and the bridge/`blockEnv`
      only ever wraps the case-test comparisons and case bodies, never the discriminant itself.
    - `lower_switch_dispatch_with_suspending_tests` — already captures the discriminant into
      `disc_var` unconditionally before any case-test matching; the bridge goes right after that
      capture, before the per-case `ConditionalGoto` chain. No discriminant-handling change needed
      here, only the bridge insertion.
    Both bridges: `ctx.scope_depth += 1`, and `ctx.states[entry_state].scope_action =
    Some(ScopeAction::OpenBlock(decls))` where `decls` is computed once (from all cases'
    `consequent`, flattened, in source order) before branching on `tests_suspend`. After the
    per-case body loop finishes (currently ending at the `break_targets` restoration), decrement
    `ctx.scope_depth -= 1` before `ctx.current_state_id = after_switch`. No `ExitScope` terminator
    is needed — same as `Block`'s non-disposing path, the generic `reconcile_scope_stack`
    (`exec.rs`) truncates the scope stack for every exit path (fallthrough, `break`, `return`,
    `throw`) by comparing the target state's static `scope_depth` to the stack's current depth,
    with no per-jump-site logic required.
    `break_target`/`ctx.loop_control_target(after_switch, ...)` is already computed earlier in
    the function (before the discriminant is touched at all), so it already captures the *outer*
    `scope_depth` — no change needed there.
- `src/interpreter/generator_analysis.rs`
  - `Statement::Switch(s) => scan_switch_body(...)` (in `scan_await_using`): change to
    `scan_flattened_list(s.cases.iter().flat_map(|c| c.consequent.iter()))`, i.e. drop the
    `switch`-specific fold entirely now that a lexical sibling next to an isolatable block is as
    safe for `switch` as it already is for `Block`/`try`.
  - Delete `scan_switch_body` (becomes dead code — it has exactly one caller) and its doc
    comment.
  - Update `scan_flattened_list`'s doc comment ("`switch` does not share this fold at all") and
    `has_suspendable_await_using_block`'s doc comment (which lists `switch` among the containers
    excluded because it "never gets its own per-entry scope") to drop the now-stale claim.
  - Checked, no change needed: the other two `Statement::Switch` arms in this file —
    `analyze_statement` (line 388, the hoisted-locals analysis pass) and `contains_yield` (line
    775). `analyze_statement`'s arm already brackets all cases with `ctx.scope_depth += 1`/`-= 1`
    symmetrically with `Statement::Block`'s arm (line 127) — this is a separate counter
    (`AnalysisContext::scope_depth`, used only to tag `local_vars` entries by nesting depth for
    name disambiguation) from `TransformContext::scope_depth` in `generator_transform.rs`, and it
    already treats `switch` as its own scope level the same way `Block` does, so there is no
    doubly-declared-binding risk here. `contains_yield`'s arm is a pure boolean walker with no
    scope bookkeeping at all.
  - Test `lowering_that_would_flatten_a_lexical_scope_is_blocked`: move
    `"switch (x) { case 1: let y = 1; case 2: { await using a = null; } }"` out of the `blocked`
    list and into `suspendable_await_using_block_through_containers`'s `isolatable` list
    (alongside the already-isolatable `Block`/`while` analogues, e.g.
    `"while (c) { let j = i; { await using a = null; } }"`). Update the surrounding doc comments
    on both tests to stop citing `switch` as an exception.
- `test262-extra/` — new regression file (see §5).

No `docs/adr/` entry: this follows the existing `#703` `ScopeAction::OpenBlock` pattern exactly
(same mechanism, new statement kind), not a new architectural decision.

## 4. TDD slices

1. **Transform-level unit test + the `transform_switch_statement` fix.**
   - Red: in `generator_transform.rs`'s `mod tests`, add a test modeled on
     `open_block_carries_post_yield_shadowing_declaration` — parse
     `"async function f() { switch (1) { case 1: let y = 1; await 0; case 2: let z = 2; } }"`
     (or equivalent via the existing `async_machine`/`transform_generator` test helpers), and
     assert exactly one state's `scope_action` is `Some(ScopeAction::OpenBlock(decls))` where
     `decls` contains **both** `y` and `z` — i.e. one scope spanning both cases, not a per-case
     one. This fails today (no state carries any `OpenBlock` at all for a `switch`).
   - Green: generalize `collect_block_lexical_decls` and add the two bridging-state insertions
     described in §3. Re-run the test.
   - Also extend the existing `test_switch_without_suspending_case_test_keeps_dispatch` /
     `test_switch_with_suspending_case_test_is_lowered` tests (or add siblings) to assert the
     bridging state's `scope_depth` is one deeper than the state before the switch and that
     `after_switch`'s recorded `scope_depth` drops back to the outer value — guards the
     depth bookkeeping directly, independent of the `decls` content.

2. **End-to-end regression test for the reported bug.**
   - Red: add `test262-extra/async-function-switch-case-block-scope.js`, modeled on
     `test262-extra/async-function-block-scope-shadowing-across-await.js`'s structure
     (`esid: sec-switch-statement-runtime-semantics-evaluation`, `flags: [async]`,
     `includes: [asyncHelpers.js]` or a manual `.then($DONE, $DONE)` chain). Cover, with each
     scenario's status against today's `main` noted explicitly so the implementer doesn't try to
     force the wrong ones red:
     - the issue's exact repro (case-local `let y` shadowing an outer `var y` across a plain
       `await`, restored after the switch) — **fails on `main`** (`["inner","inner"]` instead of
       `["inner","outer"]`).
     - a `break` variant confirming the outer binding is restored and no leak reaches code after
       the switch — **fails on `main`** for the same reason.
     - a fallthrough variant (no `break`) where a case-1 `let` is read from case 2 — **already
       passes on `main` today**, because case 1's `let` currently leaks into the enclosing
       function scope and case 2 incidentally sees it there; this scenario isn't red/green for the
       bug itself, it exists purely to catch a *wrong fix* (a per-case scope instead of one scope
       for the whole `CaseBlock`), which would make it start failing (case 2 would see a fresh
       TDZ, not case 1's initialized value) — so pin it before slice 1's production change lands,
       and treat "still passes after the fix" as the assertion.
     - a discriminant-shadowing scenario pinning which environment the discriminant evaluates in:
       `let x = 'outer'; switch (x) { case 'outer': let x = 'inner'; await 0; ...}` — **already
       passes on `main` today** (no scope exists yet to cause TDZ), but is the one scenario that
       tells apart the correct fix (discriminant evaluated in the outer env, per
       `sec-switch-statement-runtime-semantics-evaluation` step 1) from the naive fix described in
       §3's `SwitchDispatch`-branch caveat (discriminant left unbound and evaluated inside
       `blockEnv`, which would make this scenario start throwing a TDZ `ReferenceError`) — pin it
       for the same reason as the fallthrough variant, as a regression guard rather than a
       red-on-`main` case.
   - Green: the two genuinely-red scenarios should pass once slice 1 lands; the other two must
     keep passing throughout, including immediately after slice 1's production change.

3. **Relax `scan_switch_body`.**
   - Red: flip the pinned `generator_analysis.rs` test as described in §3 (move the one case from
     `blocked` to `isolatable`) — this assertion fails against `main` (today it's correctly
     blocked) and must be done *after* slices 1–2 land, not before, since relaxing the guard
     before the transform fix would reintroduce exactly this issue for that one combination.
   - Green: apply the `scan_await_using`/`scan_switch_body` changes from §3.
   - Add one end-to-end `test262-extra` case exercising that exact combination executed (not just
     statically scanned) — e.g. add a case to slice 2's new file, or a small addition to
     `test262-extra/await-using-switch-case-block-dispose-tick-alignment.js` if its existing
     `observe`/log-order harness fits — confirming the case-1 `let` and the case-2 `await using`
     block's disposal both behave correctly once the combination is no longer blocked from
     isolation.

## 5. Test surface

- The `test262` submodule is uninitialized in a fresh workspace checkout (confirmed empty here —
  `git submodule status` shows it unchecked-out); run
  `git submodule update --init --depth 1 test262` before any targeted or full test262 run below
  (same as the `spec` submodule, already needed to confirm the clause text cited in §2).
- Targeted test262 run: `uv run python scripts/run-test262.py test262/test/language/statements/switch/`
  and `uv run python scripts/run-test262.py test262/test/language/statements/async-function/`
  and `.../generators/` (switch-in-generator/async-function scoping is more likely to be covered
  under the function-kind directories than under `switch/` itself, since plain-function `switch`
  scoping was already correct via `exec_switch`'s tree-walker path and untouched by this fix).
- New tests: `test262-extra/async-function-switch-case-block-scope.js` (§4, slice 2); run via
  `uv run python scripts/run-test262.py test262-extra/async-function-switch-case-block-scope.js`.
- `cargo test --release` covers the `generator_transform.rs`/`generator_analysis.rs` unit tests
  from slices 1 and 3.
- Full gate before considering the issue closed: `uv run python scripts/run-test262.py` (full
  suite, to catch any baseline movement) and `uv run python scripts/run-custom-tests.py`.
- Not covered by test262 and needs the new `test262-extra` file: the specific interaction between
  the state-machine lowering and per-entry `CaseBlock` scoping (test262 cannot observe jsse's
  internal lowering decision; it can only observe the user-visible scoping behavior, which this
  new file exercises directly against the exact repro from the issue).

## 6. Regression risk

- **Shared machinery:** this leans entirely on the existing `ScopeAction::OpenBlock` /
  `reconcile_scope_stack` machinery `#703` already proved out for `Block`/`try`/`finally` — no
  new runtime mechanism, so the main risk is in wiring `scope_depth`/`scope_action` onto the
  right state at the right moment in `transform_switch_statement`, not in `exec.rs`'s generic
  reconciliation.
  - **Discriminant/case-test evaluation environment**: a mistake that opens the new scope before
    the discriminant is evaluated (instead of after) would change discriminant evaluation to run
    inside `blockEnv` instead of `oldEnv`, which is observable for a discriminant that references
    a name also declared `let` inside a case (TDZ) — must double check against any test262 test
    like `test262/test/language/statements/switch/scope-*` if present once the submodule is
    initialized.
  - **Per-case vs per-switch scope**: opening a fresh frame per case instead of one frame for the
    whole `CaseBlock` would silently reintroduce a different but related bug (case bodies losing
    visibility of an earlier case's `let`, e.g. on fallthrough) — slice 2's fallthrough scenario
    exists specifically to catch this.
  - **`break`/`continue`/`return`/`throw` out of a switch**: already exercised by existing
    `test262-extra/*switch*break*`/`*switch*continue*` files (e.g.
    `generator-switch-yield-free-with-break-does-not-fall-through.js`,
    `async-switch-yield-free-case-break-does-not-fall-through.js`,
    `generator-switch-yield-free-try-break-does-not-fall-through.js`) — these must keep passing
    unchanged, since they're the most likely place an off-by-one in the new bridging state's
    `Goto`/`ConditionalGoto` wiring would surface as a changed state-graph shape (wrong
    `after_state`, skipped `Goto`, etc.), independent of scoping.
  - **Suspending vs non-suspending case tests**: the fix touches both
    `lower_switch_dispatch_with_suspending_tests` and the inline `SwitchDispatch` path separately
    — `test_switch_with_suspending_case_test_is_lowered` /
    `test_switch_without_suspending_case_test_keeps_dispatch` (already in `generator_transform.rs`)
    must keep passing, and slice 1 extends both.
- **`test262-pass.txt` baseline:** this changes lowering for *every* `switch` inside *every*
  async function or generator (since any such function is always compiled through
  `generator_transform.rs`, whether or not that particular `switch` suspends) — a scoping bug in
  the new bridging logic could regress currently-passing tests broadly rather than narrowly. A
  full `scripts/run-test262.py` run (not just the targeted directories) is required before
  declaring this done, per §5.
- **Bytecode VM / tree-walker:** out of scope either way — `exec_switch` (tree-walker,
  non-generator/async functions) is unaffected; the bytecode compiler's own `switch` handling (if
  any) is a separate compilation path from `generator_transform.rs` and is not touched here.
- **GC rooting / `gc_safepoint()`:** no new allocations or root-stack interactions beyond what
  `ScopeAction::OpenBlock` already does for `Block`/`try` (a new `Environment`, already rooted by
  the existing `OpenBlock` handling in `reconcile_scope_stack`) — no change needed.

## 7. Out of scope

- **Disposal of a `using`/`await using` declared *directly* in a case** (not nested in a further
  `{ }` block) is not attempted here. `collect_block_lexical_decls` already includes such names in
  the TDZ `decls` list (for binding/shadowing correctness, same as `let`/`const`), but the plain
  `OpenBlock` path never calls `DisposeResources` — only the `EnterScope`/`ExitScope` path
  (`ctx.scopes_disposables`) does, and that path assumes a single linear statement list, which
  doesn't fit `switch`'s per-case dispatch structure. This is a pre-existing gap (today, such a
  declaration isn't disposed via any switch-aware mechanism either — it's simply treated as a
  plain binding), not a regression introduced or fixed by this change. A real fix would need its
  own design for disposal across a multi-entry-point scope and belongs in a separate issue if it
  isn't tracked already.
- **Annex B function-declaration hoisting inside a `switch` case** — already a known, tracked gap
  (`contains_annexb_function_declaration`, referenced from jsse#842 in the existing test
  comments); unaffected by this change, which only adds `OpenBlock` for `let`/`const`/class names.
- **Refactoring `transform_switch_statement`'s overall dispatch-building structure** beyond the
  minimal bridging-state insertion — the two branches
  (`lower_switch_dispatch_with_suspending_tests` vs. the inline `SwitchDispatch` path) stay
  separate; unifying them is unrelated cleanup, not needed to fix this bug.
- **Rolling `test262-pass.txt` forward** — left to `main`, per the standing rule; this plan only
  runs the suite to check for regressions, not to update the baseline.
