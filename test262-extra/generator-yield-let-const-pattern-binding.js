/*---
description: >
  A let/const declaration destructuring the result of a plain `yield` in a
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
  that the initializer's value came from a `yield` expression.
features: [generators, destructuring-binding]
---*/

function* g() {
  const { x } = yield;
  yield x;
}
var it = g();
it.next();
assert.sameValue(
  it.next({ x: 1 }).value,
  1,
  'const object pattern destructuring a plain yield result initializes the binding'
);
