# Plan: issue #709 — `await` in destructuring patterns must suspend, not drain inline

## 1. Problem restated

`generator_transform.rs` decides what to lower by asking `generator_analysis.rs` whether a
statement contains a suspension. Both look only at *expressions*: `contains_yield` /
`contains_suspension` check `VariableDeclarator::init` but never the `Pattern`, and the
`ForInOfLeft` / `CatchClause::param` arms ignore their patterns entirely. A suspension can
hide in exactly three places inside a `Pattern` (`ast.rs:289`): `Pattern::Assign(_, default)`,
`ObjectPatternProperty::KeyValue(PropertyKey::Computed(key), _)`, and
`Pattern::MemberExpression(expr)` (assignment patterns only). Statements whose only
suspension is in one of those are emitted intact and tree-walked; `bind_pattern`
(`exec.rs:1363`) evaluates the default via `eval_expr`, which reaches
`Expression::Await` (`eval.rs:1017`) → blocking `await_value` (`eval.rs:9876`), which drains
the microtask queue inline *before the async function has returned to its caller*. Result
for the issue repro: `w1,a1,sync-end,w2,w3` instead of `sync-end,w1,a1,w2,w3`.

### Probe matrix (release build of this branch, vs Node) — worse than the issue reports

| form (inside `async function f`) | jsse today |
|---|---|
| `var {a = await 1} = {}`, `let {…}`, `var [a = await 1] = []`, nested `{x:{a = await 1}}`, `{[await 1]: a}` | wrong order (`w1,a1,sync-end,…`) |
| `for (var {a = await 1} of [{}])`, `for ({a = await 1} of …)`, `catch ({a = await 5})`, async arrow, async generator (`async function*`) | wrong order |
| **`[a = await 1] = []` and `({a = await 1} = {})` (assignment expression)** | **hang (timeout, exit 124)** |
| sync generator `function* g(){ var {a = yield 1} = {}; return a }` (and `var [a = yield 1] = []`) | **silently wrong**: `{"done":true}` at once, never yields (`bind_pattern`'s `_ => UNDEFINED` swallows `Completion::Yield`) |
| sync generator assignment form `[a = yield 1] = []`, and the ~156 `dstr/*-yield-expr` / `*-yield-ident-valid` test262 cases | correct today (InlineYield replay) — **must not regress** |

Cause of the hang: for the assignment form the LHS is an *Expression* (`Array`/`Object` with
`Assign` elements), which `rewrite_expr` already rewrote `await → Yield`; `expr_has_suspension(left)`
is true, but `extract_lhs_suspensions` (`generator_transform.rs:1861`) only handles `Member` and
returns the pattern unchanged, so the emitted statement still contains a `Yield` the
async-function driver has no inline path for.

## 2. Spec basis

Slugs are `id=` values in `spec/spec.html` (grep them; the repo comments cite slugs the same way).

- `sec-runtime-semantics-keyedbindinginitialization` — *KeyedBindingInitialization*: "Let v be
  ? GetV(value, propertyName). If Initializer is present and v is undefined, then evaluate the
  Initializer and GetValue it; return ? BindingInitialization of BindingPattern with v." This is
  the clause the whole change hangs on: **one** GetV, **then** a conditional default, in property
  order, with the default evaluated only when `v` is `undefined`.
- `sec-destructuring-binding-patterns-runtime-semantics-propertybindinginitialization` —
  *PropertyBindingInitialization* (order of keys, computed key evaluated at its own position).
- `sec-runtime-semantics-bindinginitialization` (`ObjectBindingPattern`: `RequireObjectCoercible`
  first) and `sec-runtime-semantics-iteratorbindinginitialization` (array patterns: IteratorStep per
  element, `done` tracking, `IteratorClose` on abrupt completion).
- `sec-destructuring-binding-patterns-runtime-semantics-restbindinginitialization` and
  `sec-copydataproperties` (object rest excludes already-consumed keys).
- `sec-runtime-semantics-keyeddestructuringassignmentevaluation` /
  `sec-runtime-semantics-iteratordestructuringassignmentevaluation` /
  `sec-runtime-semantics-destructuringassignmentevaluation` — assignment forms; note the
  DestructuringAssignmentTarget (e.g. `o[await k]`) is evaluated *before* GetV/IteratorStep.
- `sec-runtime-semantics-catchclauseevaluation` — catch parameter BindingInitialization.
- `sec-runtime-semantics-forinofbodyevaluation-lhs-stmt-iterator-lhskind-labelset` (the id ends
  `…forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`) and
  `sec-createperiterationenvironment` — for-in/of head binding, fresh environment per iteration.
- `await` (abstract operation `Await`, 27.7.5.3) — the continuation is scheduled as a job; the
  caller of the async function continues synchronously first. This is the observable contract the
  tests assert.

## 3. Files to touch

- `src/interpreter/generator_analysis.rs` — new `pattern_contains_suspension` (+ an await-only
  twin, see §4), wired into `contains_suspension` (`Variable` arm, `ForIn`/`ForOf` `left`, `Try`
  handler `param`) and unit tests in its `tests` module.
- `src/interpreter/generator_transform.rs` — the pattern-lowering pass (new `lower_pattern_binding`
  family), hooks in `transform_variable_declaration`, `transform_for_in_of_loop`,
  `transform_try_statement`, the `Expression::Assign` arm / `extract_lhs_suspensions`; unit tests in
  its `tests` module.
- `src/parser/mod.rs` — `expr_to_pattern` (line ~1425) made `pub(crate)` if the assignment slice
  reuses it to convert cover-grammar LHS `Expression`s into `Pattern`s (otherwise write a small
  transform-local converter; do not duplicate parser validation).
- `src/interpreter/eval/generator_runtime.rs` / `src/interpreter/eval.rs` — **expected: no change**
  (transform-only fix; no new `StateTerminator`). Touch only if the catch/for-head slice proves the
  strip-to-temp rewrite needs a driver tweak; if a terminator is unavoidable, it needs
  `unreachable!()` stub arms in the other two drivers (see
  `docs/adr/2026-09-21-1007-async-block-scope-states.md`).
- `test262-extra/*.js` — new regression tests (§5).
- `docs/adr/2026-09-21-HHMM-destructuring-pattern-lowering.md` (UTC timestamp at authoring) — records
  the decision, the invariants below, and the accepted gaps. `CONTEXT.md` — add a **Pattern
  Lowering** glossary entry next to the existing state-machine terms.

## 4. Design

Only patterns that *contain a suspension* are lowered; every other pattern keeps its current
tree-walker path (zero cost, zero behaviour change). Lowering rewrites one
`pattern = value` binding into a **sequence of state-machine steps over temps**, reusing existing
machinery only (`new_temp_var`, `transform_yielding_expression` with a
`SentValueBindingKind::Variable(temp)`, `ConditionalGoto`, `Await`/`Yield` terminators). No new
`StateTerminator`.

Lowered shape for `KIND {p1 = D1, [k2]: p2, …} = V` (object pattern):

```
$src = <V, suspension-lowered as today>
<KIND> {} = $src                      // RequireObjectCoercible (empty ObjectBindingPattern)
for each property in source order:
  [key with suspension]  $k = <key expr, lowered>        // evaluated at ITS position, not hoisted up front
  $v = $src[<key or $k>]                                   // exactly ONE GetV per key
  element has no suspension:   <KIND> <element-pattern-incl-default> = $v   // tree-walker does naming/TDZ/nested
  element default suspends:    ConditionalGoto( typeof $v === "undefined" )
                                 true  -> $v = <default, lowered: Await state(s)>
                                 join  -> recurse on the element's inner pattern with source $v
  element inner pattern suspends (nested): recurse with source $v
```

Invariants the implementation and tests must state and check:

1. **Defaults are conditional.** Unlike the `Expression::Object`/`Array` *literal* arms of
   `transform_yielding_expression` (which correctly hoist every suspension unconditionally), a
   default may only run when `v` is `undefined`. If the property is present the default never runs
   and **no extra microtask tick is inserted**.
2. **One GetV per key.** The `undefined` test reads the temp (`typeof $v === "undefined"` — not the
   identifier `undefined`, which user code can shadow), never the source again. Getters fire once,
   in spec order.
3. **Computed keys are evaluated at their own position**, after earlier elements' GetV/binding and
   before their own GetV. (Hoisting all keys to the top would run an earlier getter *after* the
   key's `await`.)
4. Non-suspending siblings stay tree-walker-bound via the original sub-pattern, so
   NamedEvaluation of anonymous functions, `with` scopes, const/let kinds and nested-pattern
   semantics are unchanged.
5. `bind_pattern` is **not** restructured. Once lowering lands it never sees a suspension in a
   lowered pattern. Leave the `_ => JsValue::UNDEFINED` swallows (`exec.rs`, three sites) as they
   are: sync-generator `yield`-in-declaration-pattern still reaches them (out of scope, follow-up
   §7), so a `debug_assert!` there would fire on live inputs.

**Trigger predicate (keeps the passing yield tests on their existing path).** Lower only when
`ctx.is_async`: in a plain async function (`ctx.detect_for_await`) any suspension in the pattern
triggers (they are all awaits; note assignment-form LHS expressions were already rewritten to
`Yield` by `rewrite_expr`, patterns were not — handle both node kinds); in an async generator
trigger only on `Await` (add `expr_contains_await` mirroring `expr_contains_yield`, ~50 mechanical
lines — matching the file's existing duplication style, not a refactor), so yield-only patterns keep
the InlineYield replay path that ~156 test262 cases rely on today. Once a pattern is triggered, the
lowering handles yield and await alike. If the full-suite run shows the unified trigger regresses
nothing, widening is a follow-up, not part of this PR. Sync generators: untouched.

**Rewrite/detect/lower travel together.** A suspension in a pattern is invisible to three passes:
detection (`generator_analysis.rs`), the await→yield rewrite (`rewrite_stmt_await_to_yield` copies
`d.pattern`, `ForInOfLeft::Variable|Pattern`, and `CatchClause::param` verbatim), and lowering.
Raw `Await` in patterns is fine for lowering (`transform_yielding_expression` has an
`Expression::Await(..) if ctx.is_async` arm that emits `StateTerminator::Await` directly and
`rewrite_terminators_yield_to_await` leaves those alone) — choose *not* to rewrite patterns and say
so in a comment; but each slice below must land detection **and** lowering for its shape, or it is
either dead code or a no-op.

**Catch and for-in/of heads bind in the driver** (`EnterCatch { param }`, `ForOfHead { left }`) where
no state boundary exists. Plan a *strip-to-temp* rewrite in the transform: replace the pattern by a
`Pattern::Identifier($tmp)` in the terminator and emit the lowered binding as the first states of the
catch body / loop body, executing against the environment the driver already opened for it. Two
things the rewrite must preserve, each with a test: (a) the catch parameter's own scope (body may
`var`-redeclare only simple params; a pattern param's names must stay in the catch scope, not leak
to the function); (b) per-iteration environments for `let`/`const` heads
(`sec-createperiterationenvironment`): closures captured in iteration *n* see iteration *n*'s
pattern bindings, and a body-level `let a` shadowing a head-bound `a` must still be legal. If the
prelude cannot run in the driver-opened iteration/catch environment without a driver change, stop and
split that shape into its own follow-up rather than adding a terminator here.

**Scope of patterns:** function *parameter* patterns are **not** lowering sites — `await` in async
function formals and `yield` in generator formals are early SyntaxErrors, so
`transform_*(body, params)` never sees a suspending `params` pattern. Do not extend lowering there.

**Known/accepted gaps (documented in the ADR and PR):**
- Object rest with a suspending sibling (`{a = await 1, ...rest}`) — `CopyDataProperties` needs the
  consumed-key exclusion list without re-reading; re-destructuring or spread-then-delete would fire
  getters twice. Leave that combination on the old path (unchanged behaviour) and file a follow-up.
- TDZ precision for `let`/`const` bindings created mid-machine matches what
  `SentValueBindingKind::Pattern` (`bind_pattern(.., BindingKind::Var, func_env)`) already tolerates
  for `let {a} = await p`; not made worse.

## 5. TDD slices (each is red → green; commit per slice)

Every slice adds its test first and must show it failing on the current binary (build:
`cargo build --release -j4`, cap parallelism; run test files with a `timeout`). Run the quality
gates as **separate** commands: `cargo fmt --check`, `cargo clippy -- -D warnings`,
`cargo test --release`, `./scripts/lint.sh`.

0. **Detection plumbing (no behaviour change).** Tests in `generator_analysis.rs::tests`:
   `pattern_contains_suspension` true for default / computed key / member-expression leaf, false for
   plain patterns and for suspensions inside nested *functions*; `contains_suspension` true for
   `var {a = await 1} = {}`, `for (var {a = await 1} of x)`, `catch ({a = await 1})`. Code:
   the helper(s), `expr_contains_await`, and the `Variable`/`ForIn`/`ForOf`/`Try` arm extensions.
   Lowering hooks fall through to today's intact emission until slice 1, so all suites stay green.
1. **Object pattern, computed keys, in `var`/`let`/`const` declarations** (framework slice).
   Test: `test262-extra/async-function-destructuring-computed-key-await-order.js` — `{a, [await k]: b}`
   logs getter of `a` **before** the key's await tick and never fires it twice; issue-style tick
   interleave (`sync-end` before `w1`). Code: `lower_pattern_binding` for object patterns (source
   temp, empty-pattern RequireObjectCoercible, per-key temp, tree-walker leaf bindings), hooked in
   `transform_variable_declaration` for the `pattern` (not just `init`) case; when *both* `init` and
   pattern suspend, lower `init` into `$src` first.
2. **Defaults (the issue's headline repro).** Tests: (a) `async-function-destructuring-default-await-suspends.js`
   — exact repro from the issue, expects `sync-end,w1,a1,w2,w3`, for `var`, `let`, `const`; (b)
   `async-function-destructuring-default-not-evaluated-when-present.js` — property present ⇒ the
   default's side-effect counter stays 0, **zero extra ticks** vs. a no-default control, source getter
   count is exactly 1 (acceptance criterion for invariant 2); (c) rejecting default is caught by an
   enclosing `try/catch` and leaves the binding uninitialised. Code: `ConditionalGoto` on
   `typeof $v === "undefined"` + `transform_yielding_expression(default, Variable($v))`.
3. **Nested patterns and async-generator/arrow parity.** Tests: `{x:{a = await 1}}`, `{x:[…]}` reaches
   slice 5, object-in-object here; `async function*` and `async () =>` variants; mixed
   `await` + `yield` defaults in one async-generator pattern.
4. **Catch parameter and for-in/of heads** via strip-to-temp (see §4). Tests:
   `async-function-catch-param-default-await.js`, `async-function-for-of-head-default-await.js`,
   `…-for-in-…`, including the per-iteration-closure capture test for `let`/`const`, shadowing in the
   body, and `break`/`continue`/`return` out of the loop after the awaited default (iterator closed,
   `for_of_stack` unwound — reuse the shapes in `async-function-for-of-abrupt-completion-unwind.js`).
5. **Array patterns.** Iterator steps are observable and the await lands *between* steps
   (`[a = await 1, b = 2] = [undefined, 2]`: step, await, step); pre-stepping the iterator is not
   acceptable. Tests: step/await/step interleave with a logging iterator; holes; `...rest`; iterator
   `return()` called exactly once when the awaited default rejects; not closed when exhausted.
   Mechanism (decide in this slice, record in the ADR): keep the iterator record in function-env temps
   and drive it with a small set of interpreter-internal helpers (names that cannot lex as user
   identifiers, e.g. `%GetIteratorRecord`, `%IteratorStepValue`, `%IteratorClose`) registered where
   the async machine sets up temps, so the lowered AST calls them; abrupt exits (throw / return /
   break) must reach `IteratorClose`, most likely by reusing the `for_of_stack` unwinding the driver
   already does. If this slice grows past one reviewable change, ship slices 0–4 and file array
   patterns as its own issue.
6. **Destructuring-assignment forms (the hang).** Convert the cover-grammar LHS with
   `expr_to_pattern` and lower with the same pass, with leaf bindings as assignments; evaluate a
   member-expression target (`o[await k]`) before its GetV/IteratorStep per
   `sec-runtime-semantics-keyeddestructuringassignmentevaluation`. Tests:
   `async-function-destructuring-assignment-default-await.js` (currently times out),
   `for ({a = await 1} of …)` head, `({a = await 1} = {})`. Must keep passing: the sync-generator and
   async-generator `yield`-default assignment tests (trigger predicate, §4).
   **PR title/closing rule:** use `Closes #709` only if slices 1–6 all land; otherwise use `Refs #709`
   and file the remaining slices as follow-ups before requesting review.

Slices 1–3 alone are the minimal first PR that closes the issue's headline repro; 4–6 are the rest of
the issue's list and may be split into follow-up PRs if any of them balloons.

Final step: `git rm PLAN.md`, `cargo fmt`, ADR + `CONTEXT.md`, full gates, then push and open the PR
with a Conventional-Commits title, e.g. `fix(generators): lower await in destructuring patterns to
suspension states (#709)` (squash subject is the PR title verbatim).

## 6. Test surface

**Targeted test262 runs** (regression gate — all currently passing there must stay passing;
`uv run python scripts/run-test262.py <dir>`):
`test262/test/language/statements/for-await-of/`, `…/language/expressions/assignment/dstr/`,
`…/language/statements/for-of/dstr/`, `…/language/statements/for-in/dstr/`,
`…/language/statements/try/dstr/`, `…/language/statements/variable/dstr/`,
`…/language/statements/let/dstr/`, `…/language/statements/const/dstr/`,
`…/language/statements/async-generator/`, `…/language/expressions/async-generator/`,
`…/language/statements/async-function/`, `…/language/expressions/async-function/`,
`…/language/expressions/async-arrow-function/`, `…/language/expressions/await/`,
`…/language/module-code/top-level-await/` (guards the untouched module-level tree-walker `Await`),
`…/language/statements/generators/` and `…/language/expressions/generators/` (sync path must be
byte-for-byte unchanged), then the full default run
(`uv run python scripts/run-test262.py`) compared against the `origin/main` baseline. Never rebuild
the binary while a run is in flight; snapshot it first.

**Not covered by test262** (tick-level interleaving; test262's generated `dstr-*-init-yield-expr`
tests assert values, not job order) → `test262-extra/`, in test262 header style
(`description`, `esid:` naming the clause, `flags: [async]`, `includes: [compareArray.js]`,
`features: [async-functions, destructuring-binding]`), modelled on
`async-function-block-scope-shadowing-across-await.js`. Run with
`uv run python scripts/run-test262.py test262-extra/` (no dedicated runner). Files:

| file | esid | asserts |
|---|---|---|
| `async-function-destructuring-default-await-suspends.js` | `sec-runtime-semantics-keyedbindinginitialization` | issue repro tick order; `var`/`let`/`const` |
| `async-function-destructuring-default-not-evaluated-when-present.js` | same | default not run, zero extra ticks, getter fires once |
| `async-function-destructuring-computed-key-await-order.js` | `sec-destructuring-binding-patterns-runtime-semantics-propertybindinginitialization` | key evaluated at its position |
| `async-function-destructuring-nested-default-await.js` | `sec-runtime-semantics-bindinginitialization` | nested object/array, rejection routing |
| `async-function-catch-param-default-await.js` | `sec-runtime-semantics-catchclauseevaluation` | order, catch scope |
| `async-function-for-of-head-default-await.js` (+ for-in) | `sec-runtime-semantics-forinofbodyevaluation-…` | per-iteration envs, abrupt exit unwind |
| `async-generator-destructuring-default-await.js` | `sec-runtime-semantics-keyedbindinginitialization` | await, and mixed await+yield |
| `async-function-array-destructuring-default-await.js` | `sec-runtime-semantics-iteratorbindinginitialization` | step/await/step, close-once on rejection |
| `async-function-destructuring-assignment-default-await.js` | `sec-runtime-semantics-keyeddestructuringassignmentevaluation` | the hang; member-target order |

Rust unit tests: `generator_analysis.rs::tests` (slice 0) and `generator_transform.rs::tests` — assert
the lowered machine has an `Await` terminator and a `ConditionalGoto` for a default, has **no**
`Await` in the state that reads the property, and that a suspension-free pattern still takes the
untouched fast path (mirror `test_class_without_suspension_takes_simple_machine_fast_path`). Also run
`uv run python scripts/run-custom-tests.py` and `cargo test --release`.

## 7. Regression risk

- **Baseline (`test262-pass.txt`, read from `origin/main`)**: the ~156 `yield-expr` /
  `yield-ident-valid` dstr tests and the `async-gen-*-dstr-*` for-await-of family are the exposure —
  hence the await-only trigger in async generators and untouched sync generators. Expect zero
  removed lines; net-new passes are unlikely (this is order/scheduling, not value-visible in test262).
  Do not touch or roll the baseline file.
- **Shared machinery leaned on:** `transform_yielding_expression` (Await/Conditional arms),
  `ConditionalGoto`, scope-depth stamping (each new state gets `scope_depth` from
  `TransformContext::scope_depth` — prelude states for catch/for heads must not bump it wrongly; see
  ADR-2026-09-21-1007 and `ScopeAction`), `for_of_stack` unwind (slice 4/5), the shared
  `StateTerminator` set used by all three drivers (no new variant planned; a new one needs
  `unreachable!()` arms in the other two). Temps live in function-env `temp_vars`, already GC-rooted
  as locals; `gc_safepoint()` rooting is unchanged. Any new helper natives (slice 5) must root
  iterator records via those temps.
- **Hot paths:** `eval_expr`/`exec_statement`/`bind_pattern` are not modified. The bytecode fast path
  (`bytecode_enabled` off by default) is unaffected; still confirm the compiler bails on async
  bodies containing these shapes (`--features perf-counters` BAIL table if in doubt).
- **Perf:** patterns without suspensions add one predicate walk at transform time only.
- **Library harnesses:** run `./scripts/run-library-tests.sh zod` (async-heavy, 2,184 cases) as a
  smoke test if time allows; the others do not exercise this path.

## 8. Out of scope (do not bundle)

- Sync-generator `yield` in *declaration* patterns (`var {a = yield 1} = {}` returns immediately,
  §1 matrix) and unifying yield onto the new lowering — file a follow-up naming the silent
  wrong-value bug specifically.
- Object rest combined with a suspending sibling (needs exclusion-list `CopyDataProperties`
  primitive) unless slice 5's helpers make it trivial.
- Function *parameter* patterns (early SyntaxError for `await`/`yield`).
- Top-level-await / module-level tree-walker `Expression::Await` (`eval.rs:1017`) — still the
  legitimate blocking path there.
- Restructuring `bind_pattern`, removing the InlineYield replay backstop (issue #625), refactoring the
  duplicated `expr_contains_*` traversals, formatting or unrelated cleanups, and any
  `test262-pass.txt` update (`--update-baseline` is a `main`-only operation).
- Follow-up issues to file with `gh issue create` (non-interactive) before opening the PR, whichever of
  these were not delivered: assignment-form hang (name it a *hang*, not ordering); sync-generator
  yield in declaration patterns; array patterns; object-rest-with-suspension; catch/for-head if split.
