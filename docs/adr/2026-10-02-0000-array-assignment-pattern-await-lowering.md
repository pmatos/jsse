# Array-assignment patterns and assignment-form for-head `await` lowering

Issue #788: `%AsyncGeneratorPrototype%.return` must only enqueue while the
generator is `executing` (`sec-asyncgenerator-prototype-return` step 6) --
including while parked inside an `Await` that is not itself a `yield`.
`Await` never touches `[[AsyncGeneratorState]]`, so a construct containing
one that doesn't actually suspend through the state machine is the bug, not
the queuing logic (already correct, per
ADR-2026-09-21-2246-async-generator-awaiting-return-parking.md). Two bounded
shapes still bypassed the state machine entirely and fell into the
tree-walker's blocking `await_value` fallback, which (for a promise that
never settles) can still let unrelated queued requests observe the wrong
`[[AsyncGeneratorState]]` and settle early, since the generator never
actually suspends through a real continuation:

1. **A bare array-assignment-pattern default**, e.g. `[a = await never] =
   []`, anywhere in a generator/async-function/async-generator body.
   ADR-2026-09-22-1815 deliberately left this imprecise, reasoning it would
   need "new interpreter-internal helpers to keep a
   `GetIterator`/`IteratorStep` record alive and closed-exactly-once across
   states." That machinery already existed for the *declaration*-pattern
   case (`ArrayPatternIterOp`/`StateTerminator::ArrayPatternIter`,
   `generator_transform.rs`, built for ADR-2026-09-30-2038), so reusing it
   for the assignment form no longer required new machinery.
2. **A `for`/`for-in`/`for-of` head using the *assignment* form** (no
   `let`/`const`/`var`), e.g. `for ([a = await never] of outer) {}` or `for
   ({ b = await never } of outer) {}`. ADR-2026-09-30-2230 explicitly left
   this alone for `await` ("an array-pattern `await` default in a for-of/for-in
   head is issue #725's territory... not this one"); this ADR closes that
   gap for both object and array shapes.

## Decision

**Array-assignment patterns get the same state-machine lowering the
declaration form already has.** `lower_array_pattern_assignment`
(`generator_transform.rs`) mirrors `lower_array_pattern_binding` element for
element -- every element still costs exactly one `ArrayPatternIterOp::Step`
regardless of whether it suspends (`IteratorStepValue` order is itself
observable, issue #725), and only an element whose own target or default
reaches a suspension is broken up further. It differs from the binding form
in two ways that follow directly from `IteratorDestructuringAssignmentEvaluation`'s
`AssignmentElement` step order (spec.html:21172-21197): a leaf binds through
`lower_pattern_assignment` instead of `emit_pattern_binding` (no `kind`, no
declaration), and a member-expression leaf's reference (base, then computed
key) is captured via `lower_reference_operand` *before* that element's
`Step` -- "Left to right evaluation order is maintained by evaluating a
DestructuringAssignmentTarget that is not a destructuring pattern prior to
accessing the iterator or evaluating the Initializer." `pattern_lowering_supported`'s
`Pattern::Array` guard (`generator_analysis.rs`) now also matches
`PatternLoweringForm::Assignment`, in sync with the new lowering arm --
same shape restrictions apply (no nested array pattern was ever excluded;
object rest beside a suspending sibling remains out of scope, issue #771,
since `pattern_lowering_supported`'s `Rest` arm still requires `Declaration`
form regardless).

**An assignment-form for-head's `await`-only pattern is now detected and
hoisted, mirroring its `yield` counterpart.** `for_in_of_variable_head_contains_await`'s
`ForInOfLeft::Pattern` arm, previously hard-coded `false`, now calls
`pattern_needs_await_lowering(pattern)` -- the same `ConstrainedDeclaration`-gated
check already used for the `Variable` (declaration) arm immediately above
it. `hoist_suspending_pattern_assignment` widens its guard from
`pattern_contains_yield` alone to also fire on `pattern_needs_await_lowering`,
reusing the exact same hoist-to-body-statement strategy already used for
`yield`: `ForOfHead` sees a trivial `Pattern::Identifier`, and the real
pattern becomes a synthesized `<pattern> = <temp>;` prepended to the loop
body, where the ordinary per-statement transform -- now backed by
`lower_array_pattern_assignment` for an array shape -- picks it up and fully
decomposes it into states.

Gating the for-head hoist through `pattern_needs_await_lowering` (the
`ConstrainedDeclaration` form, which excludes a member-expression leaf) --
not the looser `pattern_needs_assignment_lowering` (the `Assignment` form,
which allows one) -- mirrors the existing `Variable`-arm precedent exactly:
an unsupported shape must stay un-hoisted, because `await` has no
InlineYield-style replay backstop the way `yield` does. A member-expression
leaf in a for-head assignment-form pattern's `await` default (e.g. `for
([o.prop = await x] of y)`) is therefore a known, narrow, deliberately
out-of-scope gap -- same shape of gap `pattern_needs_await_lowering`'s own
doc comment already documents for an object rest beside a suspending
sibling (issue #771).

## Why Trigger B depends on Trigger A

Once a for-head's assignment-form array pattern is hoisted into the body as
a synthesized `<pattern> = <temp>;` statement, that statement is lowered by
the *ordinary* per-statement transform -- which only decomposes it into
states if `pattern_needs_assignment_lowering` (the `Assignment` form) also
accepts an array shape. Without Trigger A's widening, the hoisted statement
would still fall through to `emit_pattern_assignment`'s whole-pattern
blocking-fallback call, defeating the hoist. Both land together in this PR.

## What's confirmed fixed

Both triggers were empirically verified diverging from `node` before this
change and matching it after, for: the core "a queued `.return()` during a
pending `Await` must not settle early" assertion (both a plain `async
function`'s own completion and an async generator's `.return()` racing the
pending `Await`), the array-assignment pattern's completion value, its
abrupt-close (rejected-default) iterator-close count, its elision/rest/nested
element shapes, its member-expression leaf's reference-capture ordering
(including a computed key that itself contains the suspending `await`), and
the for-head assignment-form case's outer-iterator-closing behavior (not
closed while still parked, closed exactly once once the loop is genuinely
abandoned at a real `yield`). None of this is test262-coverable: test262's
own files in this shape assert final values, not concurrent `.return()`
ordering or iterator-close counts -- all pinned as `test262-extra`
regressions instead.

## What's deliberately left alone

- **Object rest beside a suspending sibling** (`{a = await 1, ...rest} =
  x`), issue #771 -- unrelated pre-existing gap for both declaration and
  assignment forms, not reopened here.
- **A member-expression leaf in a for-head assignment-form pattern's
  `await` default** (see "Decision" above) -- narrower than #771's gap but
  the same category: `hoist_suspending_pattern_assignment`'s gate is
  conservative by design.
- **Other remaining blocking-`await_value` sites** identified during triage
  (the naive `for await` driver fallback, frame-exit disposal, loop-control-
  crossing-for-of, delegated `yield*` abrupt exits) -- each its own
  candidate issue, not reopened here; see issue #665 and the open PR/issue
  discussion for #788.
