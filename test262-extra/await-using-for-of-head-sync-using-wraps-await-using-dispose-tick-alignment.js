// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  A plain (sync-dispose) `using` for-of head has no DisposeResources Await of
  its own, but a nested `await using` block in the loop body does. The
  function must still take a real suspension-aware state for the whole
  for-of statement so that nested block's disposal Await suspends the
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
  // The for-of statement is the only suspension-worthy construct in the
  // function (no other `await` forces lowering by itself).
  var log = await observe(function (L) {
    return (async function () {
      for (using r of [{ [Symbol.dispose]() { L('disp-sync'); } }]) {
        L('body');
        { await using a = { [Symbol.asyncDispose]() { L('disp-async'); } }; }
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'disp-async', 'sync-end', 'w1', 'disp-sync', 'after', 'w2', 'settled', 'w3', 'w4'],
    'the nested await using block suspends the function at its own disposal Await'
  );

  // An unrelated `await 0` after the loop forces the whole function to
  // lower, but the for-of statement itself must still get its own
  // suspension-aware transform rather than being emitted as an opaque,
  // tree-walker-executed statement inside the lowered state.
  log = await observe(function (L) {
    return (async function () {
      for (using r of [{ [Symbol.dispose]() { L('disp-sync'); } }]) {
        L('body');
        { await using a = { [Symbol.asyncDispose]() { L('disp-async'); } }; }
      }
      L('loop-done');
      await 0;
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'disp-async', 'sync-end', 'w1', 'disp-sync', 'loop-done', 'w2', 'after', 'w3', 'settled', 'w4'],
    'the per-statement suspension transform applies even once the enclosing function is lowered for an unrelated reason'
  );

  // A lexical sibling next to the loop is not clobbered by the lowering:
  // the outer `r` stays untouched by the loop's own per-iteration binding.
  log = await observe(function (L) {
    return (async function () {
      let r = 'outer';
      {
        for (using r of [{ [Symbol.dispose]() { L('disp-sync'); } }]) {
          { await using a = { [Symbol.asyncDispose]() { L('disp-async'); } }; }
        }
      }
      L('r=' + r);
    })();
  });
  assert.compareArray(
    log,
    ['disp-async', 'sync-end', 'w1', 'disp-sync', 'r=outer', 'w2', 'settled', 'w3', 'w4'],
    'the for-of loop variable does not flatten into, or clobber, an outer lexical binding of the same name'
  );

  // Closures captured on each iteration observe distinct per-iteration
  // values of the `using` loop variable.
  log = await observe(function (L) {
    return (async function () {
      var closures = [];
      for (using r of [
        { v: 1, [Symbol.dispose]() {} },
        { v: 2, [Symbol.dispose]() {} },
      ]) {
        closures.push(function () { return r.v; });
        { await using a = { [Symbol.asyncDispose]() {} }; }
      }
      L(closures.map(function (f) { return f(); }).join(','));
    })();
  });
  assert.compareArray(
    log,
    ['sync-end', 'w1', 'w2', '1,2', 'w3', 'settled', 'w4'],
    'each iteration keeps its own per-iteration binding for the using loop variable'
  );
});
