// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-disposeresources
description: >
  An `await using` block nested in a try, catch or finally body suspends the async function at its disposal Await instead of draining the job queue inline.
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
      try {
        {
          await using a = null;
          throw 1;
        }
      } catch (e) {
        L('caught-' + e);
      }
    })();
  });
  assert.compareArray(
    log,
    ['sync-end', 'w1', 'caught-1', 'w2', 'settled', 'w3', 'w4'],
    'block in a try body that throws'
  );

  log = await observe(function (L) {
    return (async function () {
      try {
        {
          await using a = null;
          L('body');
        }
      } catch (e) {
        L('caught');
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'sync-end', 'w1', 'after', 'w2', 'settled', 'w3', 'w4'],
    'block in a try body with a catch that does not run'
  );

  log = await observe(function (L) {
    return (async function () {
      try {
        {
          await using a = null;
          L('body');
        }
      } finally {
        L('fin');
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'sync-end', 'w1', 'fin', 'after', 'w2', 'settled', 'w3', 'w4'],
    'block in a try body followed by finally'
  );

  log = await observe(function (L) {
    return (async function () {
      try {
        throw 0;
      } catch (e) {
        L('c');
        {
          await using a = null;
        }
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['c', 'sync-end', 'w1', 'after', 'w2', 'settled', 'w3', 'w4'],
    'block in a catch body'
  );

  log = await observe(function (L) {
    return (async function () {
      try {
        L('t');
      } finally {
        L('f');
        {
          await using a = null;
        }
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['t', 'f', 'sync-end', 'w1', 'after', 'w2', 'settled', 'w3', 'w4'],
    'block in a finally body'
  );
});
