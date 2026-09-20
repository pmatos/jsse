# Plan: issue #671 — `yield` (and `await`) in a switch case test is not a suspension point

## 1. Problem restated

`transform_switch_statement` (`src/interpreter/generator_transform.rs:2202`) only lowers a
suspension in the switch *discriminant*. Case test expressions are copied verbatim into
`StateTerminator::SwitchDispatch { cases: Vec<SwitchCaseTarget { test, state }> }`, and each of
the three state-machine drivers evaluates them with a plain `eval_expr` in the terminator
(sync generator `generator_runtime.rs:1438`, async generator `generator_runtime.rs:5300`,
async function `eval.rs:9016`). A `yield`/`await` in a case test is therefore never a state
boundary. `stmt_has_suspension` *does* see it (`contains_yield`/`contains_suspension` scan
`case.test`), so the switch goes through the transform and the suspension is silently
mishandled. Observed on a build of current `main` (branch #670 binary, same code):

| snippet | node | jsse today |
|---|---|---|
| `function* h(){ switch(1){ case (yield 5,1): return 'x'; } }` `[...h()]` | `[5]` | `TypeError: Iterator next failed` |
| 2-case + default generator with `case (yield 'a',1)` / `(yield 'b',2)` | `["a","b"]`, return `[2]` | nothing yielded/printed at all |
| `async function f(){ switch(1){ case await 0: …; case await 1: return 'yes' } }` | `yes` | `undefined` |
| `async function* g(){ switch(1){ case yield 'q': …; case await 1: return 'yes' } }` | yields `'q'` first | skips the yield, returns `yes` immediately |

So the bug is not limited to `yield` in sync generators: it affects `await` in async
functions and `yield`/`await` in async generators. All three share `transform_switch_statement`
(`transform_async_function` → `transform_generator_inner_opts`), so one transform-side fix covers
them.

## 2. Spec basis

- `sec-runtime-semantics-caseblockevaluation` (Runtime Semantics: CaseBlockEvaluation) —
  case selectors are evaluated **in source order**, each via `CaseClauseIsSelected`; the
  `A`-clauses run first, then the `B`-clauses (after `default`), then `default`. Since
  `default` carries no test, the overall test order is the source order of the non-default
  clauses; evaluation stops at the first selected clause, and an abrupt completion from a
  selector propagates (`? CaseClauseIsSelected`).
- `sec-runtime-semantics-caseclauseisselected` (CaseClauseIsSelected ( C, input )) —
  `exprRef = ? Evaluation of the Expression of C`, `clauseSelector = ? GetValue(exprRef)`,
  result is `IsStrictlyEqual(input, clauseSelector)`. `input` is the discriminant value
  computed once.
- `sec-switch-statement-runtime-semantics-evaluation` — the discriminant is evaluated once
  (`? GetValue`) before the CaseBlock.
- `sec-generator-function-definitions-runtime-semantics-evaluation` (`YieldExpression`) and
  `sec-async-function-definitions-runtime-semantics-evaluation` / `sec-await` — `yield`/`await`
  are valid AssignmentExpressions in a case selector and suspend the running execution context
  mid-selector; the switch resumes with the sent/awaited value as the selector value.
- `sec-try-statement-runtime-semantics-evaluation` — a throw/return injected at the suspension
  point must reach enclosing `try`/`catch`/`finally`.

(The `spec/` submodule is empty in a fresh workspace: run `git submodule update --init --depth 1 spec test262`
and confirm the anchors with `grep -rn 'id="sec-runtime-semantics-caseblockevaluation"' spec/`; the issue itself
cites `sec-runtime-semantics-caseblockevaluation` and the existing extra tests cite
`sec-switch-statement-runtime-semantics-evaluation`.)

## 3. Approach and files to touch

**Approach: lower the case dispatch to a chain of existing terminators when any case test
suspends.** No new `StateTerminator` variant and no change to the three drivers; only the
transform. The chain reuses `ConditionalGoto` (whose driver handlers already route condition
throws through the enclosing try and dispose `using` resources, #682) and the same
"hoist a suspending expression into a temp via `transform_yielding_expression` +
`SentValueBindingKind::Variable`" mechanism that `transform_if_statement`,
`transform_switch_statement` (discriminant) and `hoist_suspending_expr` already use.

Lowered shape, when `switch_stmt.cases.iter().any(|c| c.test.as_ref().is_some_and(|t| expr_has_suspension(t, ctx.is_async)))`:

1. `disc_tmp = ctx.new_temp_var("switch_disc")`. Bind the discriminant into it exactly once:
   suspending → `transform_yielding_expression(disc, ctx, usize::MAX, Some(Variable(disc_tmp)))`
   (as today); non-suspending → `emit_expression_with_binding(disc, &Some(Variable(disc_tmp)), ctx)`
   (this helper keeps an anonymous function/class discriminant from being named after the
   temp; see `test262-extra/generator-hoisted-anonymous-function-name-is-not-a-temp-name.js`).
2. Allocate the case-body states first (unchanged: one `ctx.new_state()` per clause, `default`
   recorded separately).
3. For each clause with a test, **in source order** (skip `default`):
   - suspending test → `case_tmp` (one shared temp per switch is enough, it is consumed
     immediately): `transform_yielding_expression(test, ctx, usize::MAX, Some(Variable(case_tmp)))`,
     then `ConditionalGoto { condition: Binary(StrictEq, Identifier(disc_tmp), Identifier(case_tmp)), true_state: case_state, false_state: next_test_state }`;
   - non-suspending test → `ConditionalGoto { condition: Binary(StrictEq, Identifier(disc_tmp), test.clone()), … }`
     (evaluated in the terminator like today, so identical throw routing);
   - set `ctx.current_state_id = next_test_state` (a fresh `ctx.new_state()`) after each.
4. The final `next_test_state` gets `Goto(default_state.unwrap_or(after_switch))`.
5. Case bodies, fallthrough `Goto`s, `break_targets` save/restore and `current_state_id = after_switch`
   stay as they are.

When no case test suspends (including discriminant-only suspension) the existing
`SwitchDispatch` path is kept unchanged — that keeps the hot/common shape and all existing
`generator-switch-*` / `async-generator-switch-*` tests on their current code path.

Files:

- `src/interpreter/generator_transform.rs` — `transform_switch_statement`: add the lowered
  branch (extract a helper, e.g. `lower_switch_dispatch_with_suspending_tests(...)`, to keep the
  function readable). No other production file should need changes.
- `src/interpreter/generator_transform.rs` `#[cfg(test)]` (if the file has/gets a test module) or
  `src/interpreter/tests.rs` — optional shape assertion (see slice 6).
- `test262-extra/generator-switch-case-test-yield-is-a-suspension-point.js` (new)
- `test262-extra/async-function-switch-case-test-await-is-a-suspension-point.js` (new)
- `test262-extra/async-generator-switch-case-test-yield-await-is-a-suspension-point.js` (new)
- No `docs/` / `CONTEXT.md` / ADR change: no new vocabulary or architectural decision.

Design notes / decisions to keep:

- Discriminant is captured into a temp *before* any case test runs, matching "evaluated once"
  (`sec-switch-statement-runtime-semantics-evaluation`); a case-test side effect that mutates
  the discriminant's source variable must not change `input`.
- Strict equality via `BinaryOp::StrictEq` is the same `IsStrictlyEqual` the driver's
  `strict_equality` calls, so NaN, ±0, BigInt, object identity behave identically.
- Do **not** touch the `SentValueBindingKind::InlineYield` fallback or `generator_context`
  (`generator_runtime.rs`); the fix removes this construct from the fallback's reach, nothing more.
- `with_scopes` asymmetry in the lowered shape: non-suspending links are terminator expressions
  evaluated in the driver's `term_env` (not wrapped by `with_scopes`, same as the old
  `SwitchDispatch`), whereas a suspending selector is emitted through `emit_statement` and
  `finalize_current_state` wraps it in `Statement::With` for every entry in `with_scopes`. Inside a
  `with` block a free identifier in `case a:` and in `case (yield 0, a):` can therefore resolve
  differently. Pre-existing class of limitation; do not fix or "harmonize" it here (out of scope).

## 4. TDD slices

Setup once: `git submodule update --init --depth 1 test262` (workspace has an empty submodule),
`cargo build --release -j4`. Run new files with
`uv run python scripts/run-test262.py test262-extra/<file>` (no dedicated runner for test262-extra;
pass the path). Write each test first, watch it fail on the unmodified build, then fix.

1. **Sync generator, issue repro (red → green).** New
   `test262-extra/generator-switch-case-test-yield-is-a-suspension-point.js` (esid
   `sec-runtime-semantics-caseblockevaluation`, features `[generators]`): the repro from the
   issue — `[...h()]` compares equal to `[5]` and the generator's return value is `'x'`.
   Production: add the lowered branch in `transform_switch_statement` (discriminant temp,
   one suspending-test link, default fallback). Green when that single case passes.
2. **Order, short-circuit, default position, resume values (sync generator).** Extend the same
   file: (a) tests run in source order and stop at the first match — log side effects prove
   later selectors are not evaluated; (b) `default` in the middle: tests before *and* after it
   are evaluated in source order before falling back to it, and `default` falls through into
   the following clause; (c) the value sent into `next(v)` is the selector, compared with
   `===` (`next('1')` must not select a clause for `1`; `NaN` never matches); (d) `yield*` as a
   selector; (e) non-suspending and suspending tests mixed in one switch (inline
   `disc === test` links and temp-bound links coexist); (f) discriminant is captured once even
   if a selector reassigns its source variable; (g) a suspending discriminant *and* suspending
   selectors; (h) `break`, labeled `break` to an outer label, and `continue` in an enclosing
   loop from a clause body of a lowered switch. Production: fill in remaining generality of the
   helper (multiple links, shared `case_tmp`, discriminant binding paths). Green.
3. **Abrupt completions injected at the selector suspension (sync generator).** Extend: `it.throw(e)`
   at a selector yield is caught by an enclosing `try/catch`; `it.return(v)` at a selector yield
   runs the enclosing `finally`; a throw from a *non-suspending* selector in a lowered switch
   (evaluated in a `ConditionalGoto` condition) is delivered to the enclosing catch/finally and
   later selectors are not evaluated (mirrors `generator-switch-abrupt-completions-through-try.js`
   for the lowered shape); an unhandled selector throw disposes function-level `using`
   resources before escaping. Should be green from slice 1's structure; if red, the bug is in
   how the lowered chain interacts with `try_stack` routing — fix in the transform, not the driver.
4. **Async function (`await` in a selector).** New
   `test262-extra/async-function-switch-case-test-await-is-a-suspension-point.js` (flags `[async]`,
   includes `asyncHelpers.js`, `compareArray.js`; features `[async-functions]`): `case await 0` /
   `case await 1` selects the second clause (today returns `undefined`); source order and
   short-circuit; awaited promise resolving to a matching value; a rejected selector promise is
   caught by an enclosing `try/catch` and skips later selectors; `default` in the middle. Production:
   none expected beyond slice 1 (the async-function path shares the transform; `await` is rewritten
   to yield before the transform and back to `Await` terminators after) — if red, inspect
   `rewrite_stmt_await_to_yield` (`generator_transform.rs:2442` already rewrites case tests) and
   `rewrite_terminators_yield_to_await`.
5. **Async generator (`yield` and `await` in selectors).** New
   `test262-extra/async-generator-switch-case-test-yield-await-is-a-suspension-point.js` (flags
   `[async]`; features `[async-iteration]`): `case yield 'q'` actually yields `'q'` first (today
   skipped), the value passed to `next(v)` is the selector, `case await …` inside the same
   switch, `return()`/`throw()` at a selector yield with enclosing try/finally, and a lowered
   switch inside a `for await` body with `break`/`continue`. Production: none expected.
6. **(Optional, cheap) transform-shape regression** in Rust (`cargo test --release`): assert that
   `transform_generator` on a switch whose only suspension is in a case test contains no
   `StateTerminator::SwitchDispatch`, and that a switch with only a suspending discriminant or
   only non-suspending tests still does. Guards the "keep the old path when nothing in the
   tests suspends" decision. Skip if `generator_transform.rs` has no existing test harness that
   makes this a two-line addition.
7. **Refactor pass.** Extract/inline the helper; run `./scripts/lint.sh` and the clippy/rustfmt
   hook. Do **not** route the non-lowered (`SwitchDispatch`) branch's discriminant through a temp
   or a shared "bind discriminant to temp" step: a non-suspending discriminant must stay inline
   in the `SwitchDispatch` terminator (driver `term_env`, no extra `temp_vars`, no state-body
   `with_scopes` wrapping) so the common path and its existing tests are byte-for-byte unchanged.

Commit style for the implementation stage: `fix(generators): treat yield/await in switch case
tests as suspension points` (Conventional Commits; PR title is the squash subject).

## 5. Test surface

Targeted test262 (run each with `uv run python scripts/run-test262.py <dir>`; the runner reads the
baseline from `origin/main:test262-pass.txt`, do **not** pass `--update-baseline`):

- `test262/test/language/statements/switch/`
- `test262/test/language/expressions/yield/`
- `test262/test/language/statements/generators/`, `language/expressions/generators/`
- `test262/test/language/statements/async-generator/`, `language/expressions/async-generator/`
- `test262/test/language/statements/async-function/`, `language/expressions/async-function/`,
  `language/expressions/await/`
- `test262/test/language/statements/for-await-of/`, `language/statements/for-of/` (generators
  feed these; smoke)
- `test262/test/built-ins/GeneratorPrototype/`, `built-ins/AsyncGeneratorPrototype/`
- `test262/test/annexB/language/function-code/` is only needed if `switch` function-in-block
  semantics look affected (they should not be).
- Then the full default run (`uv run python scripts/run-test262.py`) before the PR, and
  `uv run python scripts/run-test262.py test262-extra/` for every existing extra test (in
  particular the `generator-switch-*`, `async-generator-switch-*`, `async-switch-*` and
  `await-using-switch-case-*` files, which exercise the *non-lowered* path and must not move).
- `cargo test --release` and `uv run python scripts/run-custom-tests.py`.

Not covered by test262 (spec-correct behavior needing `test262-extra/`): the test262 submodule
is empty in this workspace, so coverage was not checked. After
`git submodule update --init --depth 1 spec test262`, run
`grep -rln 'case (yield\|case yield\|case await' test262/test/language/` first; where test262
already covers a scenario, treat it as targeted-run coverage instead of duplicating it. We expect it
does not cover `yield`/`await` in a `case` selector with resume values, order,
default-in-the-middle, or abrupt injection at that point — hence the three new extra files, each
naming `sec-runtime-semantics-caseblockevaluation` (plus the try-statement clause for the
injected-completion cases) in `esid`/`info`, in the standard test262 frontmatter format.

## 6. Regression risk

- Baseline (`test262-pass.txt`) movement: expected none/only additions. Only switch statements
  whose *case tests* contain a suspension change path; previously those were miscompiled, so no
  passing test should depend on the old behavior. Switches with yield-free or discriminant-only
  suspension keep the exact old `SwitchDispatch` path.
- Shared machinery leaned on: `ConditionalGoto` in all three drivers (`generator_runtime.rs:1330`,
  `:5184`, `eval.rs:8929`) — throw routing/`dispose_resources`, `try_stack` interaction, and
  `pending_exception` handling recently reworked in #682; `transform_yielding_expression` with a
  `Variable` binding; `TransformContext::new_temp_var` (temps registered in
  `GeneratorStateMachine.temp_vars` so they are declared); `clear_terminator_ic_sites` (the new
  `ConditionalGoto` conditions must stay IC-UNASSIGNED — `finalize_current_state` already does this).
- State-count growth: one extra state per case with a test (only in the lowered shape); no
  effect on generators without suspending selectors.
- Not touched: tree-walker hot paths (`eval_expr`/`exec_statement`), `property.rs`, GC
  (`gc_safepoint` roots — temps are ordinary function-env bindings, already traced), `ObjectKind`
  matches, and the bytecode fast path (`bytecode/compiler.rs` bails on `expression:Yield`
  (line ~811) and generator/async bodies run on the state machine, so a switch containing a
  suspending selector is never bytecode-compiled).
- Node-compat library harnesses: unaffected in principle; run `./scripts/run-library-tests.sh acorn`
  only if time permits (acorn/uglify use generators sparingly).

## 7. Out of scope

- Any change to the three driver `SwitchDispatch` arms (they stay for the non-suspending case);
  removing `SwitchDispatch` or replacing it entirely with the chain (possible later cleanup).
- The `InlineYield` / `generator_context` fallback in `generator_runtime.rs` (issue #625) and
  making unsupported constructs fail loudly instead of "Iterator next failed".
- The `with_scopes` asymmetry between terminator-evaluated and state-body-evaluated expressions
  (see §3 design notes), and lexical-scope flattening of
  `let`/`const` declared in case clauses versus selectors that reference them (TDZ) — pre-existing.
- Sibling bugs #669 (`try`/`with` break/continue in switch cases) and #670 (`for-in` with yield);
  other `yield` positions in already-handled statements.
- Rolling the test262 baseline forward, formatting-only changes, unrelated refactors.
