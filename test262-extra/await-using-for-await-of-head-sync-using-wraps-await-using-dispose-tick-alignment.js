// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  A `for await (using r of y)` head combines `await`-iteration (which already
  Awaits once per iteration regardless of the loop variable's own disposal)
  with a plain (sync-dispose) `using` loop variable. A nested `await using`
  block in the body still gets its own suspension-aware state, matching
  `for (using r of y)` (jsse#845), rather than draining its disposal inline.
info: |
  ForIn/OfBodyEvaluation ( lhs, stmt, iteratorRecord, iterationKind, lhsKind, labelSet [ , iteratorKind ] )

  [...]
  9.k. Else,
       i. Assert: iterationKind is iterate.
       ii. Set status to Completion(DisposeResources(iterationEnv.[[DisposeCapability]], result)).
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management, async-iteration]
---*/

function observe(shape) {
  var log = [];
  var L = function (entry) { log.push(entry); };
  Promise.resolve()
    .then(function () { L('w1'); })
    .then(function () { L('w2'); })
    .then(function () { L('w3'); })
    .then(function () { L('w4'); });
  var promise = shape(L);
  promise.then(function () { L('settled'); }, function () { L('rejected'); });
  L('sync-end');
  var drain = Promise.resolve();
  for (var i = 0; i < 12; i++) {
    drain = drain.then(function () {});
  }
  return drain.then(function () { return log; });
}

asyncTest(async function () {
  var log = await observe(function (L) {
    return (async function () {
      for await (using r of [{ [Symbol.dispose]() { L('disp-sync'); } }]) {
        L('body');
        { await using a = { [Symbol.asyncDispose]() { L('disp-async'); } }; }
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end', 'w1', 'w2', 'body', 'disp-async', 'w3', 'disp-sync', 'w4', 'after', 'settled'],
    'the nested await using block suspends the function at its own disposal Await, matching node tick-for-tick'
  );
});
