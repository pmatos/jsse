// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  A `for (await using x of iterable)` head's per-iteration DisposeResources
  suspends the async function at its disposal Await instead of draining the
  job queue inline, so the rest of the synchronous caller runs first.
info: |
  ForIn/OfBodyEvaluation ( lhs, stmt, iterator, iteratorKind, lhsKind, labelSet [ , iteratorRecordLevel ] )

  [...]
  9.j. If iterationKind is enumerate, then
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

  A witness chain of promise reactions is started before the function's
  promise gets its own reaction, so the position of "after" and "settled"
  pins the number of ticks each disposal consumed. The disposal Await of an
  iteration environment suspends the function, so the rest of the
  synchronous caller runs first.
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
      for (await using a of [null]) {
        L('body');
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'sync-end', 'w1', 'after', 'w2', 'settled', 'w3', 'w4'],
    'a resource with no dispose method needs no Await'
  );

  log = await observe(function (L) {
    return (async function () {
      for (await using a of [{ [Symbol.asyncDispose]() { L('disp'); } }]) {
        L('body');
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'disp', 'sync-end', 'w1', 'after', 'w2', 'settled', 'w3', 'w4'],
    'an async disposer suspends the function at its Await'
  );

  log = await observe(function (L) {
    return (async function () {
      for (
        await using a of [
          { [Symbol.asyncDispose]() { L('disp'); } },
          { [Symbol.asyncDispose]() { L('disp2'); } },
        ]
      ) {
        L('body');
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'disp', 'sync-end', 'w1', 'body', 'disp2', 'w2', 'after', 'w3', 'settled', 'w4'],
    'each iteration disposes its own resource before the next iteration runs'
  );

  log = await observe(function (L) {
    return (async function () {
      for (await using a of [null]) {
        L('body');
        await 0;
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'sync-end', 'w1', 'w2', 'after', 'w3', 'settled', 'w4'],
    'an await in the body ticks independently of the head disposal'
  );

  log = await observe(function (L) {
    return (async function () {
      try {
        for (
          await using a of [
            { [Symbol.asyncDispose]() { L('disp'); throw new Error('e1'); } },
            { [Symbol.asyncDispose]() { L('disp2'); } },
          ]
        ) {
          L('body');
        }
      } catch (e) {
        L('caught-' + e.message);
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'disp', 'caught-e1', 'after', 'sync-end', 'w1', 'settled', 'w2', 'w3', 'w4'],
    'a synchronously-throwing disposer needs no Await and stops the iteration'
  );

  log = await observe(function (L) {
    return (async function () {
      for (
        await using a of [
          {
            [Symbol.asyncDispose]() {
              L('disp');
              return Promise.reject(new Error('e2'));
            },
          },
        ]
      ) {
        L('body');
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'disp', 'sync-end', 'w1', 'w2', 'rejected', 'w3', 'w4'],
    'a rejecting disposer promise suspends the function, which then rejects'
  );
});
