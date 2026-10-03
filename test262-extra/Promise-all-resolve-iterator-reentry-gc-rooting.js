/*---
description: >
  Promise.all keeps the constructor's resolve method and the input iterator
  reachable across loop iterations, even when a collection is requested from
  a `then` handler invoked synchronously while settling the first element.
esid: sec-promise.all
info: |
  ECMAScript 2024 §27.2.4.1 (Promise.all) / §27.2.4.1.2 (PerformPromiseAll).

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
includes: [compareArray.js]
flags: [async]
features: [host-gc-required]
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

Promise.all.call(Sub, [1, 2, 3])
  .then(function (values) {
    assert.compareArray(values, [1, 2, 3], "all three elements are reported");
  })
  .then($DONE, $DONE);
