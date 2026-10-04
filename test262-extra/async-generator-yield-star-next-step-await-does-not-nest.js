// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-generator-function-definitions-runtime-semantics-evaluation
description: >
  The second and later `next` steps of `yield*` in an async generator suspend
  the generator at `Await(innerResult)`: a request issued from a reaction does
  not run its continuation nested inside that reaction, and requests queued
  behind a parked step settle in order.
info: |
  YieldExpression : yield * AssignmentExpression

  7. Repeat,
    a. If received.[[Type]] is normal, then
      i. Let innerResult be ? Call(iteratorRecord.[[NextMethod]], iteratorRecord.[[Iterator]], « received.[[Value]] »).
      ii. If generatorKind is async, set innerResult to ? Await(innerResult).
      [...]
      vi. If generatorKind is async, set received to Completion(AsyncGeneratorYield(? IteratorValue(innerResult))).

  Await ( value )

  Await suspends the running execution context; the continuation runs as its
  own job via PerformPromiseThen and never runs other jobs inline.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration]
---*/

asyncTest(async function () {
  var log = [];
  async function* inner() { yield 1; yield 2; yield 3; }
  async function* g() { yield* inner(); }
  var it = g();
  var p1 = it.next();
  var n2;
  p1.then(function B() {
    log.push('B-start');
    n2 = it.next().then(function (r) { log.push('n2:' + r.value); return r; });
    log.push('B-end');
  });
  p1.then(function C() { log.push('C'); });
  await p1;
  var r2 = await n2;
  assert.sameValue(r2.value, 2, 'second delegated step yields the inner value');
  assert.compareArray(
    log,
    ['B-start', 'B-end', 'C', 'n2:2'],
    'the second step does not run C inside B'
  );
  var r3 = await it.next();
  assert.sameValue(r3.value, 3, 'third step');
  assert.sameValue(r3.done, false, 'third step is not done');
  var r4 = await it.next();
  assert.sameValue(r4.done, true, 'inner exhaustion completes the generator');
});
