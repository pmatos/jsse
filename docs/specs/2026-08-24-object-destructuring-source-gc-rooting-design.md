# Object destructuring source GC rooting

## Problem

Object assignment destructuring converts its source with `ToObject`, then
retains only the resulting arena ID in a Rust local. Computed property names,
source getters, initializers, nested patterns, rest copying, and target writes
can all run JavaScript before the pattern finishes. A collection during any of
that code can sweep the source and reuse its slot, so later property reads no
longer observe the original value. Primitive sources have the same hazard
because `ToObject` creates an otherwise unreachable wrapper.

ECMAScript's `DestructuringAssignmentEvaluation` passes the same source value
through the complete `PropertyDestructuringAssignmentEvaluation`, including
each property-name evaluation and `GetV`. The implementation must keep JSSE's
object representation of that value alive for the same interval.

## Considered approaches

1. Save one temporary-root frame after `ToObject`, root the source, evaluate
   the remainder of the pattern in a closure, and unroot once after the closure
   returns. This is selected because it directly represents the specification
   lifetime and centralizes cleanup for every completion kind.
2. Root the source separately for each property. This would repeat work and
   add cleanup seams around computed names and rest properties without changing
   the required lifetime.
3. Root once and manually unroot before every existing early return. This
   would be easy to make incomplete as the evaluator changes, leaking roots on
   an overlooked abrupt or suspended completion.

## Design

`destructure_object_assignment` will continue to reject nullish sources and
perform `ToObject` before creating the root frame. Immediately after successful
conversion, it saves a frame and roots `obj_val`. The existing property loop
runs inside a closure and returns its `Completion` through that single exit.
After the closure completes, the evaluator truncates the temporary-root stack
to the saved frame and returns the captured completion.

The target-side frames inside individual properties remain unchanged. They
have a narrower purpose: preserving a pre-evaluated Reference and pending value
until `PutValue`. The new enclosing frame preserves only the source for the
whole pattern.

## Validation

Add `test262-extra` coverage that forces collection from a computed key's
`toString` and from a source getter. Exercise both ordinary object sources and
primitive string sources so newly allocated `ToObject` wrappers are covered.
Run the custom suite, the assignment-destructuring test262 directory, all Rust
quality gates, and the full test262 suite without updating the feature-branch
baseline.
