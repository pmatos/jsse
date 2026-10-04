/*---
description: >
  Promise.allSettled keeps the constructor's resolve method and the input
  iterator reachable across loop iterations, even when a collection is
  requested from a `then` handler invoked synchronously while settling the
  first element.
esid: sec-promise.allsettled
info: |
  ECMAScript 2024 §27.2.4.2 (Promise.allSettled) /
  §27.2.4.2.1 (PerformPromiseAllSettled).

  promiseResolve is obtained once via GetPromiseResolve(C) before the
  repeat-loop begins, and iteratorRecord is obtained once via GetIterator.
  Both are re-read on every iteration: promiseResolve is invoked again for
  each element and iteratorRecord.[[NextMethod]] is invoked again by
  IteratorStep. Neither value is reachable from anywhere else once the Get
  that produced it has returned, so a collection requested from inside a
  `then` handler must not free either one before a later iteration uses it.

  All three elements must be processed (and reported, in order) for the
  assertion below to hold, so a later iteration's failure (caused by a
  freed promiseResolve or iteratorRecord) is directly observable.
flags: [async]
features: [Promise.allSettled, host-gc-required]
---*/

class Sub extends Promise {}

Object.defineProperty(Sub, "resolve", {
  configurable: true,
  get: function () {
    return function (value) {
      return {
        then: function (onFulfilled) {
          if (value === 1) {
            $262.gc();
          }
          onFulfilled(value);
        },
      };
    };
  },
});

Promise.allSettled.call(Sub, [1, 2, 3])
  .then(function (records) {
    assert.sameValue(records.length, 3, "all three records are reported");
    assert.sameValue(records[0].status, "fulfilled", "record 0 is fulfilled");
    assert.sameValue(records[0].value, 1, "record 0 preserves its value");
    assert.sameValue(records[1].status, "fulfilled", "record 1 is fulfilled");
    assert.sameValue(records[1].value, 2, "record 1 preserves its value");
    assert.sameValue(records[2].status, "fulfilled", "record 2 is fulfilled");
    assert.sameValue(records[2].value, 3, "record 2 preserves its value");
  })
  .then($DONE, $DONE);
