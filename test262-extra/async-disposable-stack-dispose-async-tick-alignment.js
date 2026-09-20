// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncdisposablestack.prototype.disposeAsync
description: >
  disposeAsync returns a pending promise and settles it only after the
  Await steps of DisposeResources, at the microtask tick the spec dictates.
info: |
  AsyncDisposableStack.prototype.disposeAsync ( )

  [...]
  5. Let promiseCapability be ! NewPromiseCapability(%Promise%).
  [...]
  7. Let result be DisposeResources(asyncDisposableStack.[[DisposeCapability]], NormalCompletion(undefined)).
  8. IfAbruptRejectPromise(result, promiseCapability).
  9. Perform ! Call(promiseCapability.[[Resolve]], undefined, « result »).
  10. Return promiseCapability.[[Promise]].

  DisposeResources ( disposeCapability, completion )

  1. Let needsAwait be false.
  2. Let hasAwaited be false.
  3. For each element resource of disposeCapability.[[DisposableResourceStack]], in reverse list order, do
     [...]
     e. If method is not undefined, then
        i. Let result be Completion(Call(method, value)).
        ii. If result is a normal completion and hint is async-dispose, then
            1. Set result to Completion(Await(result.[[Value]])).
            2. Set hasAwaited to true.
     f. Else,
        i. Assert: hint is async-dispose.
        ii. Set needsAwait to true.
  4. If needsAwait is true and hasAwaited is false, then
     a. Perform ! Await(undefined).

  A witness chain of promise reactions is started before the disposal's promise
  gets its own reaction, so the position of "settled" within the witness log
  pins the number of microtask ticks DisposeResources consumed.
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
  var log = await observe(function () {
    var stack = new AsyncDisposableStack();
    stack.use(null);
    return stack.disposeAsync();
  });
  assert.compareArray(log, ['sync-end', 'w1', 'w2', 'settled', 'w3', 'w4'], 'one null resource');

  log = await observe(function () {
    var stack = new AsyncDisposableStack();
    stack.use(null);
    stack.use(undefined);
    stack.use(null);
    return stack.disposeAsync();
  });
  assert.compareArray(
    log,
    ['sync-end', 'w1', 'w2', 'settled', 'w3', 'w4'],
    'null/undefined resources share one trailing Await'
  );

  log = await observe(function () {
    return new AsyncDisposableStack().disposeAsync();
  });
  assert.compareArray(log, ['sync-end', 'w1', 'settled', 'w2', 'w3', 'w4'], 'empty stack does not Await');

  log = await observe(function (L) {
    var stack = new AsyncDisposableStack();
    stack.defer(async function () { L('disposer'); });
    return stack.disposeAsync();
  });
  assert.compareArray(
    log,
    ['disposer', 'sync-end', 'w1', 'w2', 'settled', 'w3', 'w4'],
    'async disposer runs synchronously, its Await resumes in a later job'
  );

  log = await observe(function (L) {
    var stack = new AsyncDisposableStack();
    stack.defer(async function () { L('d1'); });
    stack.defer(async function () { L('d2'); });
    return stack.disposeAsync();
  });
  assert.compareArray(
    log,
    ['d2', 'sync-end', 'w1', 'd1', 'w2', 'w3', 'settled', 'w4'],
    'each async disposer is separated by its own Await'
  );

  log = await observe(function (L) {
    var stack = new AsyncDisposableStack();
    stack.defer(function () { L('sync'); });
    return stack.disposeAsync();
  });
  assert.compareArray(
    log,
    ['sync', 'sync-end', 'w1', 'w2', 'settled', 'w3', 'w4'],
    'a synchronous disposer still leaves the promise pending until an Await ran'
  );

  log = await observe(function () {
    var stack = new AsyncDisposableStack();
    stack.defer(async function () { throw 1; });
    return stack.disposeAsync();
  });
  assert.compareArray(
    log,
    ['sync-end', 'w1', 'w2', 'rejected', 'w3', 'w4'],
    'a rejecting async disposer rejects the promise after the Await'
  );
});
