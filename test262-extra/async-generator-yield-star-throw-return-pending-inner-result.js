// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-generator-function-definitions-runtime-semantics-evaluation
description: >
  When the promise returned by the inner iterator's `throw` or `return` is
  still pending once no other job is runnable, the request waits for it rather
  than reading the result as `undefined`.
info: |
  YieldExpression : yield * AssignmentExpression

  7.b.iii.2. If generatorKind is async, set innerResult to ? Await(innerResult).
  7.c.iv. If generatorKind is async, set innerReturnResult to ? Await(innerReturnResult).

  Await suspends until the promise settles, however long that takes.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration]
---*/

asyncTest(async function () {
  var log = [];

  var releaseThrow;
  var throwGate = new Promise(function (resolve) { releaseThrow = resolve; });
  var throwIt = (async function* () {
    var r = yield* {
      [Symbol.asyncIterator]() { return this; },
      next() { return { value: 1, done: false }; },
      throw() { return throwGate; }
    };
    log.push('throw-after:' + r);
    return 'throw-end';
  })();
  await throwIt.next();
  setTimeout(function () {
    log.push('release-throw');
    releaseThrow({ value: 'T', done: true });
  }, 5);
  var thrown = await throwIt.throw('E');
  log.push('throw-settled:' + thrown.value + ':' + thrown.done);

  var releaseReturn;
  var returnGate = new Promise(function (resolve) { releaseReturn = resolve; });
  var returnIt = (async function* () {
    yield* {
      [Symbol.asyncIterator]() { return this; },
      next() { return { value: 1, done: false }; },
      return() { return returnGate; }
    };
  })();
  await returnIt.next();
  setTimeout(function () {
    log.push('release-return');
    releaseReturn({ value: 'X', done: true });
  }, 5);
  var returned = await returnIt.return('R');
  log.push('return-settled:' + returned.value + ':' + returned.done);

  assert.compareArray(
    log,
    [
      'release-throw', 'throw-after:T', 'throw-settled:throw-end:true',
      'release-return', 'return-settled:X:true'
    ],
    'throw and return steps wait for the pending inner result'
  );
});
