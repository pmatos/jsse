// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  A plain (sync-dispose) `using` for-of head has no DisposeResources Await of
  its own, but a nested `await using` block in the loop body does. An async
  generator must still take a real suspension-aware state for the whole
  for-of statement so that nested block's disposal Await suspends the
  generator instead of draining the job queue inline.
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

  A `using` (sync-dispose) head's own DisposeResources call never sets
  needsAwait, so it never performs step 4's Await. But the nested `await
  using` block's own, separate DisposeResources call does. A witness chain of
  promise reactions is started before the request promises get their own
  reactions, so the position of each marker pins the number of ticks the
  nested block's disposal consumed: it must suspend the generator, not drain
  the queue inline.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management, async-iteration]
---*/

function witnessChain(L, count) {
  var p = Promise.resolve();
  for (var i = 1; i <= count; i++) {
    (function (n) { p = p.then(function () { L('w' + n); }); })(i);
  }
  return p;
}

async function observe(shape) {
  var log = [];
  var L = function (entry) { log.push(entry); };
  witnessChain(L, 6);
  var requests = shape(L);
  requests.forEach(function (request, index) {
    request.then(
      function (r) { L('n' + index + ':' + r.value + ':' + r.done); },
      function (e) { L('n' + index + '-rej:' + (e && e.message || e)); }
    );
  });
  L('sync-end');
  var drain = Promise.resolve();
  for (var i = 0; i < 20; i++) { drain = drain.then(function () {}); }
  await drain;
  return log;
}

asyncTest(async function () {
  // The for-of statement is the only suspension-worthy construct in the
  // generator (no `yield` before it forces lowering by itself).
  var log = await observe(function (L) {
    return [(async function* () {
      for (using r of [{ [Symbol.dispose]() { L('disp-sync'); } }]) {
        L('body');
        { await using a = { [Symbol.asyncDispose]() { L('disp-async'); } }; }
      }
      L('after');
    })().next()];
  });
  assert.compareArray(
    log,
    ['body', 'disp-async', 'sync-end', 'w1', 'disp-sync', 'after', 'w2', 'n0:undefined:true', 'w3', 'w4', 'w5', 'w6'],
    'the nested await using block suspends the generator at its own disposal Await'
  );

  // An unrelated `await 0` after the loop forces the whole generator to
  // lower, but the for-of statement itself must still get its own
  // suspension-aware transform.
  log = await observe(function (L) {
    return [(async function* () {
      for (using r of [{ [Symbol.dispose]() { L('disp-sync'); } }]) {
        L('body');
        { await using a = { [Symbol.asyncDispose]() { L('disp-async'); } }; }
      }
      L('loop-done');
      await 0;
      L('after');
    })().next()];
  });
  assert.compareArray(
    log,
    ['body', 'disp-async', 'sync-end', 'w1', 'disp-sync', 'loop-done', 'w2', 'after', 'w3', 'n0:undefined:true', 'w4', 'w5', 'w6'],
    'the per-statement suspension transform applies even once the enclosing generator is lowered for an unrelated reason'
  );
});
