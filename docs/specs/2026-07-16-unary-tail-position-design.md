# Unary-expression tail-position design

## Context

AJV 8.17.1 rejects a valid meta-schema while evaluating its
`validSchemaType` helper. The helper reaches an ordinary-object branch ending
in `!Array.isArray(schema)`, but JSSE can propagate an enclosing tail-position
state into that unary operand. The call then produces a `TailCall` completion,
so the surrounding logical and unary expressions do not apply their remaining
operations. For an ordinary object, the raw `false` result escapes instead of
being negated to `true`.

ECMAScript's `HasCallInTailPosition` static semantics explicitly return
`false` for every `UnaryExpression` production. Conditional, logical, and
parenthesized expressions have separate productions that selectively preserve
tail position.

## Requirements

- Never treat a call nested in a unary expression as a tail-position call.
- Preserve proper-tail-call handling in the conditional and logical expression
  branches that the specification does classify as tail positions.
- Restore AJV's normal meta-schema compilation without an AJV-specific engine
  path or compatibility shim.
- Add spec-derived regression coverage for the expression shape that exposed
  the bug.

## Approaches considered

1. Replace the evaluator's dynamic tail-position state with parse-time
   annotations for every expression. This could make the static semantics
   explicit across the AST, but it is a broad architectural change for one
   missing exclusion.
2. Reset tail-position state at function entry or around callback execution.
   This would prevent the observed leak, but it risks disabling valid proper
   tail calls in concise bodies and nested conditional or logical branches.
3. Save and clear tail-position state while evaluating a unary operand, then
   restore it before returning. This directly implements the unary production's
   static semantics and matches the evaluator's existing scoped handling for
   conditional tests and call operands.

JSSE will use approach 3.

## Design

The `Expression::Unary` evaluation arm will save `in_tail_position`, set it to
`false` for the recursive operand evaluation, and restore the saved value on
both normal and abrupt completion paths. The unary operation then consumes the
normal operand value as before. No parser, AST, call-dispatch, or general
tail-call changes are needed.

The regression will live in `test262-extra` because test262 has positive
proper-tail-call coverage for conditional and logical expressions but no
focused negative case for a call beneath a unary operator. It will use the
AJV-shaped nested conditional/logical predicate and assert the array, ordinary
object, and object-rejects-array cases.

## Validation

- Demonstrate that the new custom regression fails before the evaluator change
  and passes after it.
- Re-run the minimal AJV 8.17.1 bundle with schema validation enabled and
  require the same `true false` validator results as Node.
- Run targeted test262 coverage for unary, conditional, logical, call, and
  return expressions to guard both the fixed exclusion and retained tail-call
  positions.
- Run the repository quality gate and full test262 suite without updating the
  feature-branch baseline.
