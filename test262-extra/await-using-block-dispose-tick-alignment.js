// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-disposeresources
description: >
  An `await using` in a block suspends the async function at the block's
  DisposeResources Await, exactly once, whether or not the function awaited
  earlier.
info: |
  DisposeResources ( disposeCapability, completion )

  [...]
  3. For each element resource of disposeCapability.[[DisposableResourceStack]], in reverse list order, do
     [...]
     f. Else,
        i. Assert: hint is async-dispose.
        ii. Set needsAwait to true.
  4. If needsAwait is true and hasAwaited is false, then
     a. Perform ! Await(undefined).

  Block : { StatementList }

  [...]
  5. Let blockValue be Completion(Evaluation of StatementList).
  6. Set blockValue to Completion(DisposeResources(blockEnv.[[DisposeCapability]], blockValue)).
  [...]

  A witness chain of promise reactions is started before the function's
  promise gets its own reaction, so the position of "after-block" and
  "settled" pins the number of ticks each disposal consumed.
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
      {
        await using a = null;
        L('body');
      }
      L('after-block');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'sync-end', 'w1', 'after-block', 'w2', 'settled', 'w3', 'w4'],
    'block without a prior Await'
  );

  log = await observe(function (L) {
    return (async function () {
      await 0;
      {
        await using a = null;
        L('body');
      }
      L('after-block');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end', 'w1', 'body', 'w2', 'after-block', 'w3', 'settled', 'w4'],
    'block after a prior Await'
  );

  log = await observe(function (L) {
    return (async function () {
      {
        await using a = { async [Symbol.asyncDispose]() { L('disposer'); } };
        L('body');
      }
      L('after-block');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'disposer', 'sync-end', 'w1', 'after-block', 'w2', 'settled', 'w3', 'w4'],
    'block with an async disposer'
  );
});
