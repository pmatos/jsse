# Async `for-of` throw scope unwind

## Problem

The async-function state-machine driver now closes every `for-of` iterator
crossed by a throw before entering its handler. However, async-function setup
also predeclares every local found by generator analysis as a function-scoped
`var`. That includes lexical bindings nested in a `for-of` head.

After the exception router removes the exited loop's iteration environment,
the catch therefore resolves the dead loop name against a synthetic function
binding and reads `undefined`. ECMA-262 `ForIn/OfBodyEvaluation` restores the
old lexical environment before returning the throw completion, so the loop
binding must instead be unresolvable in a catch outside the loop.

## Design

Apply the nested-local part of the declaration rule already used by generator
and async-generator state machines when initializing an async function:

- existing function-level storage for `var` and top-level lexical locals is
  unchanged; and
- lexical locals with nonzero analysis scope depth are not predeclared in the
  function environment because their transformed runtime scope owns them.

Preserving top-level storage is intentional. Async-function state bodies rely
on its initialized slots across suspension today; changing those slots to
lexical bindings is a separate state-machine concern and regresses deferred
module-import evaluation.

The existing async `for-of` head continues to create and initialize a fresh
iteration environment. The existing exception router continues to dispose and
remove that environment before selecting an outside catch or finally. With no
synthetic fallback binding, identifier resolution in the handler reaches the
actual outer environment and produces `ReferenceError` when appropriate.

## Alternatives considered

1. Delete or poison bindings while disposing an iteration environment. This
   would mutate an environment that closures may still retain and would not
   remove the synthetic function binding.
2. Special-case identifier resolution while a catch state executes. This
   would make lexical scope depend on control-flow state and could hide genuine
   outer bindings.
3. Add another loop-unwind call at handler entry. The current exception router
   already drains the exited loop and closes its iterator before the catch, so
   a second unwind cannot remove the function-level placeholder.

## Validation

Extend the existing async abrupt-completion regression to assert both required
observables in one scenario: the iterator `return` method runs before the catch
body, and the loop's per-iteration `const` binding is unresolvable there. Run
the focused regression, async-function and `for-of` test262 areas, the custom
suite, and the repository quality gate, followed by the full test262 suite.
