/*---
description: >
  A let/const declaration destructuring the result of `yield *` in a
  (synchronous) generator initializes the bound names instead of throwing a
  TDZ ReferenceError.
esid: sec-let-and-const-declarations-runtime-semantics-evaluation
info: |
  LexicalBinding : BindingPattern Initializer

  1. Let rhs be ? Evaluation of Initializer.
  2. Let value be ? GetValue(rhs).
  3. Return ? BindingInitialization of BindingPattern with arguments value and environment.

  BindingInitialization for a BindingPattern is performed with the running
  execution context's LexicalEnvironment as `environment`, so it goes through
  InitializeReferencedBinding rather than PutValue -- regardless of the fact
  that the initializer's value came from a `yield *` expression.
features: [generators, destructuring-binding]
---*/

function* innerConst() {
  return { x: 1 };
}
function* gConst() {
  const { x } = yield* innerConst();
  yield x;
}
var itConst = gConst();
assert.sameValue(
  itConst.next().value,
  1,
  'const object pattern destructuring a yield* result initializes the binding'
);

function* innerLet() {
  return [2];
}
function* gLet() {
  let [x] = yield* innerLet();
  yield x;
}
var itLet = gLet();
assert.sameValue(
  itLet.next().value,
  2,
  'let array pattern destructuring a yield* result initializes the binding'
);
