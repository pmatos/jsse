// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  A `for await` loop directly in an async generator body takes exactly one tick
  per Await(nextResult) with a hand-written async iterator, including the final
  pass that observes `done: true`, and performs no Promise constructor lookups.
info: |
  ForIn/OfBodyEvaluation ( lhs, stmt, iteratorRecord, iterationKind, lhsKind, labelSet )

  Repeat,
    a. Let nextResult be ? Call(iteratorRecord.[[NextMethod]], iteratorRecord.[[Iterator]]).
    b. If iteratorKind is async, set nextResult to ? Await(nextResult).
    c. If nextResult is not an Object, throw a TypeError exception.
    ...

  Await ( value )

  2. Let promise be ? PromiseResolve(%Promise%, value).
  5. Perform PerformPromiseThen(promise, onFulfilled, onRejected).

  Each pass performs one Await of a non-promise result object, so the loop over
  one element awaits twice: once for the element, once for the terminal result.
includes: [compareArray.js]
flags: [async]
features: [async-iteration]
---*/

var expected = [
  "pre",
  "tick 1",
  "loop",
  "tick 2",
  "post",
];

var actual = [];

function oneElementAsyncIterable() {
  return {
    [Symbol.asyncIterator]() {
      var done = false;
      return {
        next() {
          if (done) return { done: true, value: undefined };
          done = true;
          return { done: false, value: 0 };
        }
      };
    }
  };
}

async function* g() {
  actual.push("pre");
  for await (var x of oneElementAsyncIterable()) {
    actual.push("loop");
  }
  actual.push("post");
}

Promise.resolve(0)
  .then(() => actual.push("tick 1"))
  .then(() => actual.push("tick 2"))
  .then(() => {
    assert.compareArray(actual, expected, "Ticks and constructor lookups");
  }).then($DONE, $DONE);

Object.defineProperty(Promise.prototype, "constructor", {
  get() {
    actual.push("constructor");
    return Promise;
  },
  configurable: true,
});

g().next();
