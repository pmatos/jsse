// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-disposeresources
description: >
  An `await using` block in a loop body suspends the async function at each iteration's disposal Await instead of draining the job queue inline.
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
  promise gets its own reaction, so the position of "after" and "settled" pins
  the number of ticks each disposal consumed. The disposal Await of a nested
  block suspends the function, so the rest of the synchronous caller runs first.
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
      var i = 0;
      while (i < 2) {
        i++;
        await using a = null;
        L('b' + i);
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['b1', 'sync-end', 'w1', 'b2', 'w2', 'after', 'w3', 'settled', 'w4'],
    'while body'
  );

  log = await observe(function (L) {
    return (async function () {
      var i = 0;
      while (i < 2) {
        i++;
        {
          await using a = null;
          L('b' + i);
        }
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['b1', 'sync-end', 'w1', 'b2', 'w2', 'after', 'w3', 'settled', 'w4'],
    'while body with a nested block'
  );

  log = await observe(function (L) {
    return (async function () {
      var i = 0;
      do {
        i++;
        await using a = null;
        L('b' + i);
      } while (i < 2);
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['b1', 'sync-end', 'w1', 'b2', 'w2', 'after', 'w3', 'settled', 'w4'],
    'do-while body'
  );

  log = await observe(function (L) {
    return (async function () {
      for (var i = 0; i < 2; i++) {
        await using a = null;
        L('b' + i);
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['b0', 'sync-end', 'w1', 'b1', 'w2', 'after', 'w3', 'settled', 'w4'],
    'for (var ...) body'
  );

  log = await observe(function (L) {
    return (async function () {
      for (var x of [1, 2]) {
        await using a = null;
        L('b' + x);
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['b1', 'sync-end', 'w1', 'b2', 'w2', 'after', 'w3', 'settled', 'w4'],
    'for-of (var) body'
  );

  log = await observe(function (L) {
    return (async function () {
      var i = 0;
      outer: while (i < 2) {
        i++;
        {
          await using a = null;
          L('b' + i);
        }
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['b1', 'sync-end', 'w1', 'b2', 'w2', 'after', 'w3', 'settled', 'w4'],
    'labeled loop body'
  );

  log = await observe(function (L) {
    return (async function () {
      var i = 0;
      while (true) {
        i++;
        {
          await using a = null;
          L('b' + i);
          if (i === 1) continue;
          break;
        }
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['b1', 'sync-end', 'w1', 'b2', 'w2', 'after', 'w3', 'settled', 'w4'],
    'block with break and continue'
  );

  log = await observe(function (L) {
    return (async function () {
      for await (var x of [1, 2]) {
        await using a = null;
        L('b' + x);
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end', 'w1', 'w2', 'b1', 'w3', 'w4', 'b2', 'after', 'settled'],
    'for await body'
  );
});
