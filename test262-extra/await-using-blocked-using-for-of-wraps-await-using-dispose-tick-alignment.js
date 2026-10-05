// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  A `using` (sync-dispose) for-of head has no DisposeResources Await of its
  own, but a nested `await using` block in its body does. A sibling
  sloppy-mode `function` declaration — in the same statement list as the
  for-of, whether that's the for-of's own body or an enclosing block —
  independently blocks lowering per Annex B
  (sec-web-compat-functiondeclarationinstantiation), downgrading the reach
  from `Isolatable` to `Blocked`. When this `Blocked` reach is the function's
  *only* suspension point, the function must still take a real
  suspension-aware state so the nested block's disposal Await suspends the
  function instead of draining the job queue inline.
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
  promise reactions is started before the function's promise gets its own
  reaction, so the position of "after" and "settled" pins the number of
  ticks the nested block's disposal consumed: it must suspend the function,
  not drain the queue inline.
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
  // The issue's literal repro shape: the sibling function declaration sits
  // in the *enclosing* block, alongside the for-of statement (not inside the
  // for-of's own body). That still reaches the outer block's own
  // Annex-B guard (scan_scoped_list's unsafe-flatten check), downgrading the
  // whole block from Isolatable to Blocked.
  var log = await observe(function (L) {
    return (async function () {
      {
        for (using r of [{ [Symbol.dispose]() { L('disp-sync'); } }]) {
          { await using a = { [Symbol.asyncDispose]() { L('disp-async'); } }; }
        }
        function g() {}
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['disp-async', 'sync-end', 'w1', 'disp-sync', 'after', 'w2', 'settled', 'w3', 'w4'],
    'a sibling function declaration in the enclosing block blocks the for-of reach but must still suspend the function'
  );

  // The sibling function declaration sits directly inside the for-of's own
  // body instead, which blocks via blocked_by_annexb_sibling on the for-of
  // statement's own classification.
  log = await observe(function (L) {
    return (async function () {
      for (using r of [{ [Symbol.dispose]() { L('disp-sync'); } }]) {
        { await using a = { [Symbol.asyncDispose]() { L('disp-async'); } }; }
        function g() {}
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['disp-async', 'sync-end', 'w1', 'disp-sync', 'after', 'w2', 'settled', 'w3', 'w4'],
    'a sibling function declaration inside the for-of body blocks the loop itself but must still suspend the function'
  );
});
