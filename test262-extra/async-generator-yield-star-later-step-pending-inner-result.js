// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-generator-function-definitions-runtime-semantics-evaluation
description: >
  When the promise returned by the inner iterator's `next` at a later
  `yield*` step is still pending once no other job is runnable, the request
  waits for it rather than reading the result as `undefined`.
info: |
  YieldExpression : yield * AssignmentExpression

  7.a.ii. If generatorKind is async, set innerResult to ? Await(innerResult).

  Await suspends until the promise settles, however long that takes.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration]
---*/

asyncTest(async function () {
  var log = [];
  var release;
  var gate = new Promise(function (resolve) { release = resolve; });
  var n = 0;
  var innerIt = {
    [Symbol.asyncIterator]() { return this; },
    next() { n++; return n === 1 ? { value: 1, done: false } : gate; }
  };
  async function* g() {
    var r = yield* innerIt;
    log.push('after:' + JSON.stringify(r));
  }
  var it = g();
  var p1 = it.next().then(function (r) { log.push('n1:' + r.value + ':' + r.done); });
  var p2 = it.next().then(function (r) { log.push('n2:' + r.value + ':' + r.done); });
  setTimeout(function () {
    log.push('release');
    release({ value: 'x', done: true });
  }, 5);
  await Promise.all([p1, p2]);
  assert.compareArray(
    log,
    ['n1:1:false', 'release', 'after:"x"', 'n2:undefined:true'],
    'the second step waits for the pending inner result'
  );
});
