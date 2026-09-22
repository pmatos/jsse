# Async destructuring-*assignment* patterns: no more manufactured `Yield`

Issue #724: `[a = await 1] = []` and `({a = await 1} = {})` hung a plain
`async function` forever. The left side of a destructuring-assignment cover
grammar is still an `Expression` (`Array`/`Object` with `Assign` elements) at
the point the async-function transform runs, not a `Pattern` — the parser
only reparses a cover-grammar left side into `Pattern` for declarations and
`for`-head positions. `rewrite_expr` walked the whole function body and
rewrote every `Await` to `Yield` unconditionally, including inside that
`Expression`, but `lower_reference_operand` (the only thing that inspected an
assignment's left operand) only lowers a `Member` reference and returned the
`Array`/`Object` unchanged. The emitted state kept a bare `Yield` node a plain
async function's driver has no inline path to service — the future never
settled.

## Decision

**Never manufacture the `Yield` in the first place.** Narrow `rewrite_expr`'s
`Expression::Assign` arm so an `Array`/`Object` left side under `=` is left
untouched, the same way a `Pattern` already is. An `await` inside it now
survives as a real `Await` node, which either:

- gets lowered into proper suspension states (object patterns — below), or
- runs through the pre-existing blocking-`await_value` tree-walker path when
  it reaches `eval_expr` as part of an atomic statement — the same fallback
  ADR-2026-09-21-2143 already accepts for declaration-side array patterns.
  Either way, the function no longer hangs.

**Object destructuring-assignment patterns get the same state-machine
lowering ADR-2026-09-21-2143 gave declaration patterns.**
`lower_pattern_assignment`/`lower_pattern_assignment_property`
(`generator_transform.rs`) are the assignment-form twins of
`lower_pattern_binding`/`lower_pattern_property`: same per-property,
source-order walk following `KeyedDestructuringAssignmentEvaluation`
(`sec-runtime-semantics-keyeddestructuringassignmentevaluation`), but a leaf
resolves to a plain assignment-expression statement instead of a
`Statement::Variable`, and the pattern is reached by converting the
cover-grammar `Expression` with `expr_to_pattern` (now `pub(crate)` in
`parser/mod.rs`, alongside `pattern_to_expr` for the reverse direction).
`pattern_needs_assignment_lowering` (`generator_analysis.rs`) gates entry,
parallel to the declaration-only `pattern_needs_lowering`; both share
`pattern_lowering_supported`, now parameterized by
`allow_member_expression` — an assignment can target a member expression
(`o[k] = ...`), a declaration never can.

**A member-expression leaf's reference is captured before the property
read.** `KeyedDestructuringAssignmentEvaluation` evaluates a non-pattern
target's reference (`lRef`) *before* the source `GetV`, so
`lower_pattern_assignment_property` detects a `Pattern::MemberExpression`
leaf (after unwrapping any `Pattern::Assign` default) and captures its
base/computed-key via `lower_reference_operand`'s existing reference-freezing
idiom first, in source order, before reading `source[key]` and applying the
default.

**The assignment expression's completion value is the RHS value, not the
destructured result** (`AssignmentExpression` step 7.c: "Return rval"). The
new lowering binds the source temp — already holding that value — to the
caller's `binding` at the end; the pre-existing blocking-fallback path
already got this right for free, since it evaluates the intact
`Expression::Assign` and binds *that* expression's own result.

## What this change deliberately leaves imprecise

- **Array assignment patterns' job-ordering precision.** After this change
  `[a = await 1] = []` no longer hangs and produces the correct value, but
  still runs through the blocking-`await_value` fallback rather than true
  state-machine lowering, so its relative ordering against other
  concurrently-scheduled jobs is not spec-perfect — the same accepted
  imprecision ADR-2026-09-21-2143 already documents for declaration array
  patterns. Giving array assignment patterns a real iterator record held
  across suspension states is materially larger (new interpreter-internal
  helpers to keep a `GetIterator`/`IteratorStep` record alive and
  closed-exactly-once across states) and is left as a follow-up.
- **Object rest beside a suspending sibling** (`{a = await 1, ...rest} = x`)
  stays unsupported by `pattern_lowering_supported`, same as the declaration
  form.
- **`for (... of ...)`/`for (... in ...)` heads were never actually
  affected.** Empirically verified on this branch:
  `async function f(){ var a; for ({a = await 1} of [{}]) {} return a }`
  already resolved to `1` before this change, because the parser reparses a
  `for`-of/`for`-in left side into `Pattern` before `generator_transform.rs`
  ever sees it (`src/parser/statements.rs:1174`, `:1199`), so it never reaches
  the `rewrite_expr` `Expression::Assign` arm this change narrows. The
  `for`-head repro in the issue body did not reproduce on this revision; see
  the issue comment left alongside this fix.
