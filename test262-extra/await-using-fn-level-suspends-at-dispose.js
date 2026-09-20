// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-disposeresources
description: >
  An `await using` at function-body level suspends the async function at each
  Await of DisposeResources instead of running queued jobs inline, and the
  function's promise settles at the tick the spec dictates.
info: |
  DisposeResources ( disposeCapability, completion )

  1. Let needsAwait be false.
  2. Let hasAwaited be false.
  3. For each element resource of disposeCapability.[[DisposableResourceStack]], in reverse list order, do
     a. Let value be resource.[[ResourceValue]].
     b. Let hint be resource.[[Hint]].
     c. Let method be resource.[[DisposeMethod]].
     d. If hint is sync-dispose and needsAwait is true and hasAwaited is false, then
        i. Perform ! Await(undefined).
        ii. Set needsAwait to false.
     e. If method is not undefined, then
        i. Let result be Completion(Call(method, value)).
        ii. If result is a normal completion and hint is async-dispose, then
            1. Set result to Completion(Await(result.[[Value]])).
            2. Set hasAwaited to true.
        [...]
     f. Else,
        i. Assert: hint is async-dispose.
        ii. Set needsAwait to true.
  4. If needsAwait is true and hasAwaited is false, then
     a. Perform ! Await(undefined).

  Await suspends the running execution context, so no job may run in the
  middle of the async function's synchronous prologue. A witness chain of
  promise reactions is started before the function's promise gets its own
  reaction, so the position of "settled" pins the number of ticks consumed.
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
      await using a = null;
      L('body');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'sync-end', 'w1', 'w2', 'settled', 'w3', 'w4'],
    'a null resource costs one Await after the body'
  );

  log = await observe(function (L) {
    return (async function () {
      await using a = null;
      await using b = undefined;
      await using c = null;
      L('body');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'sync-end', 'w1', 'w2', 'settled', 'w3', 'w4'],
    'null and undefined resources share one trailing Await'
  );

  log = await observe(function (L) {
    return (async function () {
      await using a = { async [Symbol.asyncDispose]() { L('disposer'); } };
      L('body');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'disposer', 'sync-end', 'w1', 'w2', 'settled', 'w3', 'w4'],
    'an async disposer runs synchronously and its Await suspends the function'
  );

  log = await observe(function (L) {
    return (async function () {
      await using a = null;
      L('body');
      throw 1;
    })();
  });
  assert.compareArray(
    log,
    ['body', 'sync-end', 'w1', 'w2', 'rejected', 'w3', 'w4'],
    'a throw completion is delivered after the disposal Await'
  );

  log = await observe(function (L) {
    return (async function () {
      await using a = null;
      L('body');
      return 7;
    })();
  });
  assert.compareArray(
    log,
    ['body', 'sync-end', 'w1', 'w2', 'settled', 'w3', 'w4'],
    'a return completion is delivered after the disposal Await'
  );

  log = await observe(function (L) {
    return (async function () {
      await using a = { async [Symbol.asyncDispose]() { L('d1'); } };
      await using b = { async [Symbol.asyncDispose]() { L('d2'); } };
      L('body');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'd2', 'sync-end', 'w1', 'd1', 'w2', 'w3', 'settled', 'w4'],
    'each async disposer is separated by its own Await'
  );

  log = await observe(function (L) {
    return (async function () {
      await using a = { async [Symbol.asyncDispose]() { throw 'a'; } };
      L('body');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'sync-end', 'w1', 'w2', 'rejected', 'w3', 'w4'],
    'a rejecting async disposer rejects the function promise after the Await'
  );

  log = await observe(function (L) {
    return (async function () {
      using s = { [Symbol.dispose]() { L('sync-disposed'); } };
      await using n = null;
      L('body');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'sync-end', 'w1', 'sync-disposed', 'w2', 'settled', 'w3', 'w4'],
    'the Await for a null resource happens before a following sync disposer runs'
  );

  log = [];
  Promise.resolve().then(function () { log.push('microtask'); });
  (async function () {
    await using a = null;
  })();
  log.push('sync');
  assert.compareArray(log, ['sync'], 'no queued job runs inside the synchronous prologue');
});
