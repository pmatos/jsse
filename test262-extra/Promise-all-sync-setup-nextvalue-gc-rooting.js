/*---
description: >
  Promise.all keeps its result capability and accumulated values reachable
  while advancing the input iterator on a later iteration.
esid: sec-promise.all
info: |
  ECMAScript 2024 §27.2.4.1.2 (PerformPromiseAll), repeat-loop step 1.

  IteratorStepValue can invoke user code on every iteration. A custom resolve
  method can also invoke the element handler synchronously, leaving earlier
  values reachable only through the combinator accumulator when a later next()
  call requests a collection.
flags: [async]
features: [host-gc-required, Symbol.iterator]
---*/

class Sub extends Promise {}

Object.defineProperty(Sub, "resolve", {
  configurable: true,
  value: function (value) {
    return {
      then: function (onFulfilled) {
        onFulfilled(value);
      },
    };
  },
});

var callCount = 0;
var iterable = {};

iterable[Symbol.iterator] = function () {
  return {
    next: function () {
      callCount += 1;
      if (callCount === 2) {
        $262.gc();
      }
      if (callCount <= 3) {
        return {
          done: false,
          value: { marker: "value-" + callCount },
        };
      }
      return { done: true };
    },
  };
};

Promise.all.call(Sub, iterable)
  .then(function (values) {
    assert.sameValue(values.length, 3, "all values are reported");
    assert.sameValue(values[0].marker, "value-1", "the first value survives");
    assert.sameValue(values[1].marker, "value-2", "the second value survives");
    assert.sameValue(values[2].marker, "value-3", "the third value survives");
  })
  .then($DONE, $DONE);
