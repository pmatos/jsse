// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: An awaited async iterator result must be an Object, including when it is undefined
info: |
  ForIn/OfBodyEvaluation ( lhs, stmt, iteratorRecord, iterationKind, lhsKind, labelSet [ , iteratorKind ] )

  8. Repeat,
     a. Let nextResult be ? Call(iteratorRecord.[[NextMethod]], iteratorRecord.[[Iterator]]).
     b. If iteratorKind is async, set nextResult to ? Await(nextResult).
     c. If nextResult is not an Object, throw a TypeError exception.
flags: [async]
includes: [asyncHelpers.js]
features: [Symbol.asyncIterator, async-iteration]
---*/

asyncTest(async function () {
  var calls = 0;
  var iterator = {
    [Symbol.asyncIterator]() {
      return this;
    },
    next() {
      calls += 1;
      return Promise.resolve(calls === 1 ? undefined : { done: true });
    },
  };
  await assert.throwsAsync(TypeError, async function () {
    for await (var value of iterator) {}
  });
  assert.sameValue(calls, 1, 'the invalid first result terminates iteration');
});
