/*---
description: >
  Promise.race keeps its newly created capability reachable while retrieving
  the constructor's resolve method.
esid: sec-promise.race
info: |
  ECMAScript 2024 §27.2.4.5 (Promise.race) steps 2–3.

  NewPromiseCapability(C) precedes GetPromiseResolve(C). The resolve property
  access can invoke user code and request a collection, before either
  capability function has been installed as a promise reaction.
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

Promise.race.call(Sub, [1, 2])
  .then(function (value) {
    assert.sameValue(value, 1, "the first settlement wins");
  })
  .then($DONE, $DONE);
