/*---
description: >
  Promise combinators keep their newly created capability reachable while
  retrieving the constructor's resolve method.
esid: sec-promise.all
info: |
  ECMAScript 2024 §27.2.4.1 (Promise.all) steps 2–3.

  NewPromiseCapability(C) creates the result promise and its resolving
  functions before GetPromiseResolve(C) performs Get(C, "resolve"). The Get can
  invoke user code, including a host-requested collection. The capability must
  remain reachable throughout that synchronous setup window.
flags: [async]
features: [host-gc-required]
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

var combined = Promise.all.call(Sub, [1, 2]);

combined
  .then(function (values) {
    assert.compareArray(values, [1, 2], "the capability survives synchronous setup");
  })
  .then($DONE, $DONE);
