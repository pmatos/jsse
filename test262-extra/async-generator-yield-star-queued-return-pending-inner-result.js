// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgeneratorunwrapyieldresumption
description: >
  A `return()` request queued behind a `next()` that is parked at a `yield*`
  step waits for the pending promise returned by the inner iterator's `return`.
info: |
  AsyncGeneratorYield ( value )

  11. If queue is not empty, then
    a. Let toYield be the first element of queue.
    b. Let resumptionValue be Completion(toYield.[[Completion]]).
    c. Return ? AsyncGeneratorUnwrapYieldResumption(resumptionValue).

  YieldExpression : yield * AssignmentExpression

  7.c.iv. If generatorKind is async, set innerReturnResult to ? Await(innerReturnResult).
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration]
---*/

asyncTest(async function () {
  var log = [];
  var release;
  var gate = new Promise(function (resolve) { release = resolve; });
  var it = (async function* () {
    yield* {
      [Symbol.asyncIterator]() { return this; },
      next() { return { value: 1, done: false }; },
      return() { return gate; }
    };
  })();
  var p1 = it.next().then(function (r) { log.push('n1:' + r.value + ':' + r.done); });
  var p2 = it.return('R').then(function (r) { log.push('ret:' + r.value + ':' + r.done); });
  setTimeout(function () {
    log.push('release');
    release({ value: 'x', done: true });
  }, 5);
  await Promise.all([p1, p2]);
  assert.compareArray(
    log,
    ['n1:1:false', 'release', 'ret:x:true'],
    'the queued return request waits for the inner return result'
  );
});
