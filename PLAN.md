# Plan: issue #724 — `await` in a destructuring *assignment* pattern hangs the async function

## 1. Problem restated

`async function f(){ var a; [a = await 1] = []; return a }` (and the object form
`({a = await 1} = {})`) hangs a plain `async function` forever instead of resolving.
The left side of `[a = await 1] = []` is the `AssignmentPattern` cover grammar —
still an `Expression::Array`/`Expression::Object` AST node at the point
`generator_transform.rs` processes the function body, not a `Pattern`. Because it is
an `Expression`, the async-function-body rewrite pass (`rewrite_expr`,
`generator_transform.rs:3120`) recurses into it and turns the nested `await 1` into
`Yield(Some(1), false)` — the same textual transform applied to every other `Await`
in the body. But the statement containing that `Assign` is never split into
suspension states for it: `Expression::Assign`'s handling in
`transform_yielding_expression` (`generator_transform.rs:1347`) routes the left
operand through `lower_reference_operand` (`generator_transform.rs:2018`), which only
lowers a `Member` reference and returns anything else — including this whole
`Array`/`Object` — unchanged. The emitted state therefore contains a raw `Yield` node
inside an otherwise ordinary statement, executed as one atomic tree-walker step. A
plain `async function`'s driver has no inline-replay path for a bare `Yield` reached
this way (unlike sync/async generators, which do — see Regression risk below), so
the driver waits for a resume signal that its own state machine never produces:
the future never settles and `f().then(...)` never fires.

## 2. Spec basis

- **`sec-destructuring-assignment`** (Destructuring Assignment, supplemental
  grammar): `AssignmentPattern` covers `ObjectAssignmentPattern` /
  `ArrayAssignmentPattern`; `DestructuringAssignmentTarget : LeftHandSideExpression`.
  This is why the left side of `[a = await 1] = []` is legitimately an `Expression`
  (`Array`/`Object`) at parse time, not a `Pattern` — jsse's parser only reparses a
  cover-grammar left side into `Pattern` for declarations and `for`-head positions
  (`src/parser/statements.rs:1174`, `:1199`), never for a plain `AssignmentExpression`.
- **`sec-runtime-semantics-destructuringassignmentevaluation`**: for
  `ObjectAssignmentPattern : { AssignmentPropertyList }`,
  `RequireObjectCoercible(value)` then
  `PropertyDestructuringAssignmentEvaluation`. The `{ }` case (empty pattern) is
  `RequireObjectCoercible` alone — matches the existing `<kind> {} = $src` idiom
  `lower_pattern_binding` already emits for declarations.
- **`sec-runtime-semantics-propertydestructuringassignmentevaluation`**, production
  `AssignmentProperty : PropertyName : AssignmentElement`: evaluate the property
  name, then `KeyedDestructuringAssignmentEvaluation`.
- **`sec-runtime-semantics-keyeddestructuringassignmentevaluation`**, production
  `AssignmentElement : DestructuringAssignmentTarget Initializer?`: **(1)** if the
  target is not an `ObjectLiteral`/`ArrayLiteral` (i.e. it is a plain
  `LeftHandSideExpression` — identifier or member expression), evaluate its
  reference (`lRef`) *first*; **(2)** `GetV(value, propertyName)`; **(3)** if
  `Initializer` is present and the read is `undefined`, evaluate the initializer;
  **(4)** if the target is itself an object/array literal, recurse into
  `DestructuringAssignmentEvaluation`, otherwise `PutValue(lRef, rhsValue)`. Step
  order (1) before (2) is the "member-expression target evaluated before its
  `GetV`" requirement the issue calls out for `o[await k]`-shaped targets.
- **`sec-runtime-semantics-restdestructuringassignmentevaluation`**: rest target's
  `lRef` (if not itself object/array literal) is evaluated before
  `CopyDataProperties`. Not touched by this plan — rest stays out of scope for
  lowering, same restriction `lower_pattern_binding` already has for declarations
  (`pattern_lowering_supported`, `generator_analysis.rs:959-963`).
- **`sec-runtime-semantics-iteratordestructuringassignmentevaluation`**
  (`ArrayAssignmentPattern`): each element additionally needs a live
  `GetIterator`/`IteratorStep`/`IteratorClose` record threaded through the
  suspension points. This is the array-pattern case, deferred — see §7.
- General assignment evaluation (`AssignmentExpression : LeftHandSideExpression =
  AssignmentExpression`, object/array-literal branch): `rval` is `GetValue` of the
  right side, evaluated *before* `DestructuringAssignmentEvaluation` runs, and the
  whole expression's value is `rval`. This is why the lowering must still bind the
  source temp to the expression's own `SentValueBindingKind` — `[a = await
  1] = []` used as a value (`x = ([a] = [])`, `return [a] = []`) must produce the
  right-hand value, not `undefined`.

## 3. Files to touch

Engine only; no tooling/CI/benchmark changes.

- `src/interpreter/generator_transform.rs`
  - `rewrite_expr` (`Expression::Assign` arm, ~3172): stop recursing the
    `Await`→`Yield` rewrite into the left operand when it is an `Array`/`Object`
    cover-grammar destructuring target under a plain `=`. Leaves it exactly as
    written (raw `Await`/`Yield`, whatever the source had), the same way `Pattern`
    nodes already escape this rewrite (`sec-destructuring-assignment` note in the
    existing ADR).
  - `transform_yielding_expression`'s `Expression::Assign` arm (~1347): when the
    left operand parses as an `Object` `Pattern` that needs lowering (see below),
    convert the RHS into a source temp and lower the pattern into assignment
    statements instead of falling through to `lower_reference_operand` +
    `emit_expression_with_binding` on the intact expression. Bind the source
    temp's value to the caller's `binding` afterward (the expression's completion
    value).
  - New: `lower_pattern_assignment(pattern: &Pattern, source: &str, ctx: &mut
    TransformContext)` and `lower_pattern_assignment_property(...)` — assignment
    twins of `lower_pattern_binding` / `lower_pattern_property` (~1839-1931).
    Differences from the declaration twins: leaves emit `Statement::Expression(
    Expression::Assign(AssignOp::Assign, target_expr, value_expr))` instead of
    `Statement::Variable`; a `Pattern::MemberExpression` leaf's reference (base +
    computed key, via the same temp-capture idiom `lower_reference_operand` uses)
    is evaluated *before* the property read, per step order in
    `KeyedDestructuringAssignmentEvaluation`.
- `src/interpreter/generator_analysis.rs`
  - `pattern_lowering_supported` (~954): parameterize (or add a sibling) so a
    `Pattern::MemberExpression` leaf is accepted for the assignment form (it is
    unconditionally rejected today, which is correct for the declaration form —
    you cannot declare into a member expression — but is exactly what
    assignment-form lowering must support). `Pattern::Array` and
    `ObjectPatternProperty::Rest` stay rejected for both forms (deferred, §7).
  - Add `pattern_needs_assignment_lowering(pattern: &Pattern) -> bool` mirroring
    `pattern_needs_lowering` (~973), trigger unchanged: `pattern_contains_await`,
    not `pattern_contains_suspension` — a `yield`-only pattern must keep using the
    existing InlineYield replay path (see Regression risk).
- `src/parser/mod.rs`
  - `expr_to_pattern` (~1425) and `pattern_to_expr` (~1529): change from private
    `fn` to `pub(crate) fn` so `generator_transform.rs` can reuse them instead of
    re-implementing cover-grammar-to-`Pattern` conversion. By the time
    `generator_transform.rs` sees this `Expression::Assign`, the parser has
    already accepted it as a valid `AssignmentPattern` (or the program would have
    been a `ParseError`), so the `Result::Err` arm is unreachable in practice;
    treat it defensively as "leave the expression intact" (fall through to
    today's behavior), not a panic.
- `docs/adr/2026-09-21-2143-destructuring-pattern-lowering.md`: append a one-line
  "Superseded in part by ADR-<new-date>" pointer (existing convention, e.g. the
  note in `2026-09-21-2300-yield-star-delegated-step-suspension.md:40`).
- New `docs/adr/<today>-destructuring-assignment-lowering.md`: record (a) the
  `rewrite_expr` narrowing and why it is safe (patterns already skip this rewrite;
  this makes the assignment cover-grammar case behave the same way), (b) the
  object-assignment lowering being modeled on `lower_pattern_binding`, (c) the
  deliberate scope cut leaving `ArrayAssignmentPattern` on the pre-existing
  blocking-`await_value` fallback (no hang, but the same job-ordering imprecision
  the original ADR already accepts for declaration array patterns and for-head
  patterns), and (d) that `for (... of ...)`/`for (... in ...)` heads are
  unaffected — the parser already reparses those into `Pattern` at
  `src/parser/statements.rs:1174`/`:1199`, so they never hit `rewrite_expr`'s
  `Expression::Assign` arm and were not actually hanging (verified empirically;
  see §7).
- `test262-extra/`: new files, see §5.

## 4. TDD slices

1. **Un-hang: stop rewriting `Await`→`Yield` inside a destructuring-assignment
   left side.**
   - Red: a `tests/` or inline unit test in `generator_transform.rs`'s existing
     `#[cfg(test)]` module asserting `rewrite_expr` on
     `Assign(Assign, Array([Some(Assign(Assign, Identifier("a"),
     Await(Literal(1))))]), Array([]))` leaves the `Await` un-rewritten (today it
     asserts/observes a `Yield`).
   - Green: narrow the `Expression::Assign` arm of `rewrite_expr` as described in
     §3.
   - Follow-up assertion in the same slice: the two hanging repros from the issue
     body now terminate. Add `test262-extra/async-function-destructuring-assignment-array-default-await.js`
     and `.../async-function-destructuring-assignment-object-default-await.js`
     asserting the *value* (`a === 1`) — this is the literal issue repro and is
     genuinely spec-correct output even though job-ordering-vs-other-microtasks
     is not yet perfected for the array case (§7).
   - This slice alone closes the hang for all three repros in the issue body
     (array assignment, object assignment, and — per the empirical check in §7 —
     the `for (... of ...)` head was never actually broken).

2. **Object-assignment lowering: correct ordering, no blocking drain.**
   - Red: `test262-extra/async-function-destructuring-assignment-computed-key-order.js`
     — an object assignment pattern with two computed keys, one of which
     `await`s, asserting each key's `GetV`/property-read side effect fires in
     source order relative to a job scheduled between them (mirrors
     `async-function-destructuring-computed-key-await-order.js`'s existing
     declaration-side test, adapted to the assignment form). This fails before
     the lowering exists (falls back to blocking `await_value`, wrong relative
     order) — Red state confirmed by running it against slice 1 only.
   - Green: add `pattern_needs_assignment_lowering` /
     `pattern_lowering_supported`'s parameterization (`generator_analysis.rs`),
     expose `expr_to_pattern`/`pattern_to_expr` (`parser/mod.rs`), and implement
     `lower_pattern_assignment`/`lower_pattern_assignment_property`
     (`generator_transform.rs`), wired from the `Expression::Assign` arm.
   - Refactor: factor the shared "conditional default, `typeof v ===
     'undefined'`" state-pair construction out of `lower_pattern_property` and
     the new `lower_pattern_assignment_property` if the duplication is more than
     a few lines (both need the identical `ConditionalGoto` + join-state shape).

3. **Member-expression target ordering (`o[await k] = ...` inside an object
   pattern).**
   - Red: `test262-extra/async-function-destructuring-assignment-member-target-order.js`
     — object pattern whose value position is a computed member expression
     (`{a: obj[await idx()]} = src`), asserting the base/key are evaluated (and
     the member reference captured) before the property's `GetV`, mirroring
     `test262-extra/destructuring-assignment-member-target-suspension.js`'s
     structure but for `await` in a plain async function instead of `yield` in a
     sync generator.
   - Green: extend `lower_pattern_assignment_property`'s leaf case to detect a
     `Pattern::MemberExpression` target, capture its base/key via the same
     idiom `lower_reference_operand` uses, before reading `source[key]`.

4. **Completion value.**
   - Red: `test262-extra/async-function-destructuring-assignment-expression-value.js`
     — asserts `(async () => { var a; return [a = await 1] = []; })()` resolves
     to `[]` (the RHS array, not `undefined` or `1`), and the object form
     resolves to the RHS object.
   - Green: bind the source temp to `binding` at the end of the new
     `Expression::Assign` handling path (§3).

## 5. Test surface

- **test262, targeted re-run (regression, must stay green):**
  - `test262/test/language/expressions/assignment/dstr/` (13 `*-yield-expr.js`
    files here specifically) — the assignment-form sync-generator `yield`-default
    tests; must keep passing via the *unchanged* trigger
    (`pattern_contains_await`, not `pattern_contains_suspension`) so they never
    enter the new lowering path.
  - `test262/test/language/statements/for-of/dstr/`,
    `test262/test/language/statements/for-in/dstr/`,
    `test262/test/language/statements/for-await-of/` — `*-yield-expr*` (`for`-head
    forms; unaffected by this change per §7, but re-run as a baseline check since
    they share `expr_to_pattern`/`pattern_to_expr`, now made `pub(crate)`).
  - `test262/test/language/expressions/assignment/` and
    `test262/test/language/statements/async-function/`,
    `test262/test/staging/` (if any `dstr`/assignment/async interaction exists) —
    broad targeted run to catch anything `rewrite_expr`'s narrower recursion
    might have missed.
  - Full baseline: `uv run python scripts/run-test262.py` (regression check
    against `origin/main:test262-pass.txt`, not updating it — implementation
    stage only).
- **test262-extra (new, spec-correct behavior test262 does not cover — confirmed
  by search: no `test262/test/language/expressions/assignment/dstr/*-await-expr.js`
  file exists today):**
  - `async-function-destructuring-assignment-array-default-await.js` (slice 1)
  - `async-function-destructuring-assignment-object-default-await.js` (slice 1)
  - `async-function-destructuring-assignment-computed-key-order.js` (slice 2)
  - `async-function-destructuring-assignment-member-target-order.js` (slice 3)
  - `async-function-destructuring-assignment-expression-value.js` (slice 4)
  - Each follows the existing `test262-extra/async-function-destructuring-*.js`
    file pattern: an `esid`/`info` header citing the exact spec clause(s) from
    §2, then `assert`/`assert.sameValue` on observable behavior.
- **Existing regression sentinels, must stay green:**
  `test262-extra/destructuring-assignment-member-target-suspension.js` (sync
  generator, `yield`, unaffected — different trigger and no `rewrite_expr`
  involvement), `async-generator-destructuring-default-await.js`,
  `async-function-destructuring-default-await-with-scope.js`,
  `async-function-destructuring-default-await-suspends.js`,
  `async-function-destructuring-nested-default-await.js`,
  `async-function-destructuring-computed-key-await-order.js`,
  `async-function-destructuring-default-not-evaluated-when-present.js`.
- `cargo test --release` for the `generator_transform.rs`/`generator_analysis.rs`
  unit tests (existing `#[cfg(test)]` modules) plus the new one from slice 1.

## 6. Regression risk

- **`test262-pass.txt` baseline**: the `rewrite_expr` narrowing only changes
  behavior when the `Expression::Assign` left operand is literally an
  `Array`/`Object` under `AssignOp::Assign` — every other assignment shape
  (`Identifier`, `Member`, compound ops) is untouched, and detection
  (`expr_has_suspension`/`stmt_has_suspension`) is unaffected because it already
  treats `Await` and `Yield` interchangeably (`generator_analysis.rs:797`). Risk
  is concentrated in the new lowering path (`lower_pattern_assignment*`), which
  only activates when `pattern_contains_await` is true *and*
  `pattern_lowering_supported` accepts the shape — same activation discipline
  `lower_pattern_binding` already uses safely for #709.
- **The InlineYield/replay fallback asymmetry** (empirically verified, not just
  inferred from the issue text): a raw `Yield` mid-statement is handled by the
  sync-generator and async-generator drivers via `GeneratorContext` replay
  (`src/interpreter/eval/generator_runtime.rs`, `InlineYield` binding kind), but
  the plain-`async function` driver has no equivalent — this is *why* the bug is
  a hang specifically for `async function`, not for `async function*`/`function*`.
  Confirmed: `async function* g(){ var a; [a = await 1] = []; yield a; }` driven
  through `for await` does **not** hang on this branch, while the equivalent
  plain `async function` does. The fix must not change generator/async-generator
  behavior — it only changes what a plain `async function`'s state ever contains,
  by not manufacturing a `Yield` node there in the first place. Add (or confirm
  test262/test262-extra already covers) an async-generator counterpart of each
  new test to pin this asymmetry down as a regression sentinel.
- **`for`-head forms were not actually broken.** Empirically verified on this
  branch: `async function f(){ var a; for ({a = await 1} of [{}]) {} return a }`
  already resolves to `1`, because the parser reparses the `for`-of/`for`-in left
  side into `Pattern` before `generator_transform.rs` ever sees it
  (`src/parser/statements.rs:1174`, `:1199`), so it never reaches the
  `rewrite_expr` `Expression::Assign` arm this plan changes. **This is a
  documented judgment call, diverging from the issue body's third repro claim**
  (`for ({a = await 1} of …)`  is listed there as hanging); the implementation
  stage should `gh issue comment 724` noting the empirical result before closing,
  in case the reporter observed a different jsse revision or a subtly different
  repro.
- **Shared machinery**: `expr_to_pattern`/`pattern_to_expr` becoming `pub(crate)`
  is a visibility change only, no behavior change to existing callers
  (`parser/statements.rs`, `parser/expressions.rs`). `lower_reference_operand`
  and `lower_pattern_binding` (declaration form) are not modified — the new
  functions are additive siblings, so #709's declaration-side behavior and its
  tests are structurally unreachable from this change.
- **GC rooting**: the new lowering introduces temp vars (`dstr_src`, `dstr_key`,
  `dstr_val`-equivalents, matching #709's naming) holding `JsValue`s across
  states; these live in `ctx.temp_vars`, already rooted as ordinary function
  locals (per the existing ADR's "No new `StateTerminator`" note) — no new GC
  root-scope work needed as long as the assignment lowering reuses the same
  temp-var mechanism rather than raw Rust locals.
- **Bytecode fast path**: async functions with any suspension already fall back
  to the tree-walker/state-machine (`bytecode_enabled` gate); this change does
  not touch `bytecode/`.

## 7. Out of scope

- **Array assignment patterns' job-ordering precision**
  (`sec-runtime-semantics-iteratordestructuringassignmentevaluation`): after
  slice 1, `[a = await 1] = []` no longer hangs and produces the correct value,
  but still runs through the blocking-`await_value` tree-walker fallback rather
  than true state-machine lowering, so its relative ordering against other
  concurrently-scheduled jobs is not spec-perfect. This is the same accepted
  imprecision the existing ADR already documents for declaration array patterns
  and for-head patterns; giving array assignment patterns a real iterator record
  held across suspension states is a materially larger change (new
  interpreter-internal helpers to keep a `GetIterator`/`IteratorStep` record
  alive and closed-exactly-once across states) and does not belong in this bug
  fix. Left as a follow-up.
- **Object rest beside a suspending sibling** (`{a = await 1, ...rest}` in
  assignment form): stays unsupported by `pattern_lowering_supported`, same as
  the declaration form; `CopyDataProperties` needs the consumed-key exclusion
  list threaded through, unchanged problem from the existing ADR.
- **`catch ({a = await 5})` and any other binding sites not already covered by
  #709 or this plan**: not exercised by this issue's repros, not touched.
- **Rewriting/refactoring `lower_pattern_binding` or `lower_reference_operand`
  to share code with the new assignment twins beyond what slice 2's refactor
  step calls out**: the declaration and assignment forms differ enough
  (statement kind, member-expression-target support, evaluation-order rules)
  that forcing a single shared implementation now would be premature
  abstraction; revisit only if a later issue needs a third variant.
- **Formatting/lint cleanups** anywhere in `generator_transform.rs`,
  `generator_analysis.rs`, or `parser/mod.rs` beyond the lines this plan touches.
