/*---
description: >
  Promise.all keeps its result capability reachable while retrieving the input
  iterator.
esid: sec-promise.all
info: |
  ECMAScript 2024 §27.2.4.1 (Promise.all) steps 2 and 5.

  NewPromiseCapability(C) creates the result capability before GetIterator
  retrieves iterable[Symbol.iterator]. That property access can invoke user
  code and request a collection, but the capability must remain reachable.
flags: [async]
features: [host-gc-required, Symbol.iterator]
---*/

var iterable = {};

Object.defineProperty(iterable, Symbol.iterator, {
  configurable: true,
  get: function () {
    $262.gc();
    return function () {
      var done = false;
      return {
        next: function () {
          if (done) {
            return { done: true };
          }
          done = true;
          return { done: false, value: 42 };
        },
      };
    };
  },
});

Promise.all(iterable)
  .then(function (values) {
    assert.sameValue(values.length, 1, "one value is reported");
    assert.sameValue(values[0], 42, "the iterator value is preserved");
  })
  .then($DONE, $DONE);
