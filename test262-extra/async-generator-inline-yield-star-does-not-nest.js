// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-generator-function-definitions-runtime-semantics-evaluation
description: >
  A `yield*` inside an expression the state-machine transform does not lower
  (here a destructuring-assignment default) suspends the async generator at
  `Await(innerResult)`: jobs queued before the request are not drained inside
  it, and the request settles after the same number of ticks as the lowered
  form.
info: |
  YieldExpression : yield * AssignmentExpression

  7. Repeat,
    a. If received.[[Type]] is normal, then
      i. Let innerResult be ? Call(iteratorRecord.[[NextMethod]], iteratorRecord.[[Iterator]], « received.[[Value]] »).
      ii. If generatorKind is async, set innerResult to ? Await(innerResult).

  Await ( value )

  Await suspends the running execution context; the continuation runs as its
  own job via PerformPromiseThen and never runs other jobs inline.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration, destructuring-assignment]
---*/

async function* inlineDelegate() { var a, b; ({a = yield 1, b = yield* [10]} = {}); }
async function* loweredDelegate() { var a, b; yield 1; yield* [10]; }

async function trace(makeGenerator) {
  var log = [];
  var it = makeGenerator();
  await it.next();
  var chain = Promise.resolve();
  for (var i = 1; i <= 8; i++) {
    (function (n) { chain = chain.then(function () { log.push('w' + n); }); })(i);
  }
  var second = it.next('sent').then(function (r) {
    log.push('next-resolved:' + r.value);
    return r;
  });
  log.push('after-next');
  await second;
  await chain;
  return log;
}

asyncTest(async function () {
  var inlineLog = await trace(inlineDelegate);
  var loweredLog = await trace(loweredDelegate);
  assert.sameValue(inlineLog[0], 'after-next', 'the request does not run queued jobs before returning');
  assert.compareArray(inlineLog, loweredLog, 'inline yield* settles on the same ticks as a lowered yield*');
});
