/*---
description: >
  Promise.any keeps its newly created capability reachable while retrieving
  the constructor's resolve method.
esid: sec-promise.any
info: |
  ECMAScript 2024 §27.2.4.3 (Promise.any) steps 2–3.

  NewPromiseCapability(C) precedes GetPromiseResolve(C). The resolve property
  access can invoke user code and request a collection, but the result
  capability remains live throughout synchronous setup.
flags: [async]
features: [Promise.any, AggregateError, host-gc-required]
---*/

class Sub extends Promise {}

Object.defineProperty(Sub, "resolve", {
  configurable: true,
  get: function () {
    $262.gc();
    return function (value) {
      return Promise.resolve(value);
    };
  },
});

Promise.any.call(Sub, [1, 2])
  .then(function (value) {
    assert.sameValue(value, 1, "the first fulfillment wins");
  })
  .then($DONE, $DONE);
