/*---
description: >
  Promise.allSettled keeps its newly created capability reachable while
  retrieving the constructor's resolve method.
esid: sec-promise.allsettled
info: |
  ECMAScript 2024 §27.2.4.2 (Promise.allSettled) steps 2–3.

  NewPromiseCapability(C) precedes GetPromiseResolve(C). The resolve property
  access can invoke user code and request a collection, but the result
  capability remains live throughout synchronous setup.
flags: [async]
features: [Promise.allSettled, host-gc-required]
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

Promise.allSettled.call(Sub, [1, 2])
  .then(function (records) {
    assert.sameValue(records.length, 2, "both records are reported");
    assert.sameValue(records[0].status, "fulfilled", "record 0 is fulfilled");
    assert.sameValue(records[0].value, 1, "record 0 preserves its value");
    assert.sameValue(records[1].status, "fulfilled", "record 1 is fulfilled");
    assert.sameValue(records[1].value, 2, "record 1 preserves its value");
  })
  .then($DONE, $DONE);
