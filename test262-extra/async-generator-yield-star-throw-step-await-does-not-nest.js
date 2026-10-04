// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-generator-function-definitions-runtime-semantics-evaluation
description: >
  The `throw` step of `yield*` in an async generator suspends the generator at
  `Await(innerResult)`: a `throw()` issued from a reaction does not run its
  continuation nested inside that reaction, and the delegated result is
  handled according to `done` and `IteratorValue`.
info: |
  YieldExpression : yield * AssignmentExpression

  7.b. Else if received.[[Type]] is throw, then
    [...]
    iii. If throw is not undefined, then
      1. Let innerResult be ? Call(throw, iteratorRecord.[[Iterator]], « received.[[Value]] »).
      2. If generatorKind is async, set innerResult to ? Await(innerResult).
      [...]
      5. Let done be ? IteratorComplete(innerResult).
      6. If done is true, then
        a. Return ? IteratorValue(innerResult).
      7. If generatorKind is async, set received to Completion(AsyncGeneratorYield(? IteratorValue(innerResult))).

  Await ( value )

  Await suspends the running execution context; the continuation runs as its
  own job via PerformPromiseThen and never runs other jobs inline.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration]
---*/

function mk(throwImpl) {
  return {
    [Symbol.asyncIterator]() { return this; },
    next() { return Promise.resolve({ value: 'n', done: false }); },
    throw: throwImpl
  };
}

asyncTest(async function () {
  var log = [];
  var it = (async function* () {
    yield* mk(function (e) { return Promise.resolve({ value: 't:' + e, done: false }); });
  })();
  var p1 = it.next();
  var thr;
  p1.then(function B() {
    log.push('B-start');
    thr = it.throw('E').then(function (r) { log.push('thr:' + r.value); return r; });
    log.push('B-end');
  });
  p1.then(function C() { log.push('C'); });
  await p1;
  var r = await thr;
  assert.sameValue(r.value, 't:E', 'the throw step yields the inner value');
  assert.sameValue(r.done, false, 'the generator is still delegating');
  assert.compareArray(
    log,
    ['B-start', 'B-end', 'C', 'thr:t:E'],
    'the throw step does not run C inside B'
  );

  var doneIt = (async function* () {
    var r = yield* mk(function (e) { return Promise.resolve({ value: 'T:' + e, done: true }); });
    yield 'after:' + r;
  })();
  await doneIt.next();
  var afterDone = await doneIt.throw('E');
  assert.sameValue(afterDone.value, 'after:T:E', 'a done throw result becomes the yield* value');
  assert.sameValue(afterDone.done, false, 'and the generator continues');

  var err = new Error('value getter');
  var caughtIt = (async function* () {
    try {
      yield* mk(function () { return Promise.resolve({ done: false, get value() { throw err; } }); });
    } catch (e) {
      yield 'caught:' + (e === err);
    }
  })();
  await caughtIt.next();
  var caught = await caughtIt.throw('E');
  assert.sameValue(caught.value, 'caught:true', 'a throwing IteratorValue is catchable by the generator');
});
