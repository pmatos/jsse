// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-with-statement-runtime-semantics-evaluation
description: >
  A `with` statement whose body directly declares `await using` suspends the
  enclosing async generator at the block's DisposeResources Await, exactly
  like an unwrapped `await using` block, instead of draining the disposal
  inline (issue #862).
info: |
  WithStatement : with ( Expression ) Statement

  [...]
  4. Let newEnv be NewObjectEnvironment(obj, true, oldEnv).
  5. Set the running execution context's LexicalEnvironment to newEnv.
  6. Let C be Completion(Evaluation of Statement).
  [...]

  DisposeResources ( disposeCapability, completion )

  [...]
  3. For each element resource of disposeCapability.[[DisposableResourceStack]], in reverse list order, do
     [...]
     f. Else,
        i. Assert: hint is async-dispose.
        ii. Set needsAwait to true.
  4. If needsAwait is true and hasAwaited is false, then
     a. Perform ! Await(undefined).

  A witness chain of promise reactions is started before the generator's
  first request is settled, so the position of "after" and "settled" pins
  the number of ticks the disposal consumed.
flags: [async, noStrict]
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
  var unwrapped = await observe(function (L) {
    return (async function* () {
      {
        await using a = { [Symbol.asyncDispose]() { L('disp-async'); } };
      }
      L('after');
    })().next();
  });
  var wrapped = await observe(function (L) {
    return (async function* () {
      with ({}) {
        await using a = { [Symbol.asyncDispose]() { L('disp-async'); } };
      }
      L('after');
    })().next();
  });
  assert.compareArray(
    unwrapped,
    ['disp-async', 'sync-end', 'w1', 'after', 'w2', 'settled', 'w3', 'w4'],
    'baseline: an unwrapped await-using block suspends the generator at disposal'
  );
  assert.compareArray(
    wrapped,
    unwrapped,
    'disposal of an await-using declared directly inside a with-body suspends the generator instead of draining inline'
  );
});

asyncTest(async function () {
  var log = await observe(function (L) {
    return (async function* () {
      with ({}) {
        try {
          await using a = { [Symbol.asyncDispose]() { L('disp-async'); } };
        } finally {
          L('fin');
        }
      }
      L('after');
    })().next();
  });
  assert.compareArray(
    log,
    ['disp-async', 'sync-end', 'w1', 'fin', 'after', 'w2', 'settled', 'w3', 'w4'],
    'a try-body await-using inside a with-body suspends at disposal, so the finally block runs one tick later'
  );
});
