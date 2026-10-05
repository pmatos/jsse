// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  An `await using` for-of head wrapping a nested `await using` block in its
  body suspends at both disposal Awaits instead of draining the job queue
  inline for either one. `scan_await_using`'s ForOf arm classifies this shape
  Blocked (the head variable itself can't be isolated into a per-entry
  scope), but the independent `stmt_contains_await_using_head` gate already
  forces full state-machine lowering whenever the head itself `disposes_at_head`,
  so the nested block still gets correct suspension even though the scan
  classification doesn't change for it.
info: |
  ForIn/OfBodyEvaluation ( lhs, stmt, iteratorRecord, iterationKind, lhsKind, labelSet [ , iteratorKind ] )

  [...]
  9.k. Else,
       i. Assert: iterationKind is iterate.
       ii. Set status to Completion(DisposeResources(iterationEnv.[[DisposeCapability]], result)).

  DisposeResources ( disposeCapability, completion )

  [...]
  3. For each element resource of disposeCapability.[[DisposableResourceStack]], in reverse list order, do
     [...]
     f. Else,
        i. Assert: hint is async-dispose.
        ii. Set needsAwait to true.
  4. If needsAwait is true and hasAwaited is false, then
     a. Perform ! Await(undefined).

  Both the head's own per-iteration DisposeResources and the nested block's
  separate DisposeResources set needsAwait (both hints are async-dispose), so
  each must suspend the function at its own Await. A witness chain of promise
  reactions is started before the function's promise gets its own reaction,
  so the position of "after" and "settled" pins the number of ticks each
  disposal consumed.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management]
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
      for (await using r of [{ [Symbol.asyncDispose]() { L('disp-outer'); } }]) {
        L('body');
        { await using a = { [Symbol.asyncDispose]() { L('disp-inner'); } }; }
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'disp-inner', 'sync-end', 'w1', 'disp-outer', 'w2', 'after', 'w3', 'settled', 'w4'],
    'the nested block disposes and suspends before the head\'s own per-iteration disposal, which suspends again'
  );
});
