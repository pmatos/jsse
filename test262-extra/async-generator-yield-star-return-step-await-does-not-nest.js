// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-generator-function-definitions-runtime-semantics-evaluation
description: >
  The `return` step of `yield*` in an async generator suspends the generator at
  `Await(innerReturnResult)`: a `return()` issued from a reaction does not run
  its continuation nested inside that reaction, and the delegated result is
  handled according to `done`.
info: |
  YieldExpression : yield * AssignmentExpression

  7.c. Else,
    [...]
    iii. Let innerReturnResult be ? Call(return, iteratorRecord.[[Iterator]], « received.[[Value]] »).
    iv. If generatorKind is async, set innerReturnResult to ? Await(innerReturnResult).
    [...]
    vii. Let done be ? IteratorComplete(innerReturnResult).
    viii. If done is true, then
      1. Let value be ? IteratorValue(innerReturnResult).
      2. Return ReturnCompletion(value).
    ix. If generatorKind is async, set received to Completion(AsyncGeneratorYield(? IteratorValue(innerReturnResult))).

  Await ( value )

  Await suspends the running execution context; the continuation runs as its
  own job via PerformPromiseThen and never runs other jobs inline.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration]
---*/

function mk(returnImpl) {
  return {
    [Symbol.asyncIterator]() { return this; },
    next() { return Promise.resolve({ value: 'n', done: false }); },
    return: returnImpl
  };
}

asyncTest(async function () {
  var log = [];
  var it = (async function* () {
    yield* mk(function (v) { return Promise.resolve({ value: 'r:' + v, done: false }); });
  })();
  var p1 = it.next();
  var ret;
  p1.then(function B() {
    log.push('B-start');
    ret = it.return('R').then(function (r) { log.push('ret:' + r.value); return r; });
    log.push('B-end');
  });
  p1.then(function C() { log.push('C'); });
  await p1;
  var r = await ret;
  assert.sameValue(r.value, 'r:R', 'the return step yields the inner value');
  assert.sameValue(r.done, false, 'the generator is still delegating');
  assert.compareArray(
    log,
    ['B-start', 'B-end', 'C', 'ret:r:R'],
    'the return step does not run C inside B'
  );

  var doneIt = (async function* () {
    yield* mk(function (v) { return Promise.resolve({ value: 'X:' + v, done: true }); });
  })();
  await doneIt.next();
  var finished = await doneIt.return('R');
  assert.sameValue(finished.value, 'X:R', 'a done return result becomes the return value');
  assert.sameValue(finished.done, true, 'and completes the generator');
  var after = await doneIt.next();
  assert.sameValue(after.done, true, 'the generator stays completed');
});
