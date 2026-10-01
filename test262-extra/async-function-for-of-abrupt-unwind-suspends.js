// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  When an async function's `for (await using x of iterable)` loop is
  abruptly exited (throw, return, or break/continue crossing the loop) and
  the loop's per-iteration environment holds a resource whose
  [Symbol.asyncDispose] itself awaits, closing that iteration environment
  suspends the async function at the disposal's Await instead of draining
  the job queue inline.
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

  A witness chain of promise reactions is started before the async function's
  own result promise gets its reaction, so the position of each marker pins
  the number of ticks the disposal consumed. The disposal Await suspends the
  async function (rather than draining the job queue inline), so the rest of
  the synchronous caller runs first.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management, async-functions]
---*/

function witnessChain(L, count) {
  var p = Promise.resolve();
  for (var i = 1; i <= count; i++) {
    (function (n) { p = p.then(function () { L('w' + n); }); })(i);
  }
  return p;
}

async function observe(run) {
  var log = [];
  var L = function (entry) { log.push(entry); };
  witnessChain(L, 6);
  var request = run(L);
  request.then(
    function (r) { L('resolved:' + r); },
    function (e) { L('rejected:' + (e && e.message || e)); }
  );
  L('sync-end');
  var drain = Promise.resolve();
  for (var i = 0; i < 20; i++) { drain = drain.then(function () {}); }
  await drain;
  return log;
}

function resource(L, name) {
  return { [Symbol.asyncDispose]() { L('disp' + name); } };
}

function rejectingResource(L, name, message) {
  return {
    [Symbol.asyncDispose]() {
      L('disp' + name);
      return Promise.reject(new Error(message));
    }
  };
}

asyncTest(async function () {
  // (a) an uncaught throw unwinding a single `await using`-bound loop
  // exercises the top-level throw-routing block's
  // `unwind_async_for_of_loops` call.
  async function uncaughtThrow(L) {
    for (await using a of [resource(L, '')]) { L('body'); throw new Error('boom'); }
  }
  var log = await observe(function (L) { return uncaughtThrow(L); });
  assert.compareArray(
    log,
    ['body', 'disp', 'sync-end', 'w1', 'w2', 'rejected:boom', 'w3', 'w4', 'w5', 'w6'],
    'an uncaught throw unwinding the loop suspends at the resource disposal Await'
  );

  // (b) a return doing the same exercises `route_return!`'s
  // `unwind_for_of!` call.
  async function returnAcrossLoop(L) {
    for (await using a of [resource(L, '')]) { L('body'); return 'ret'; }
  }
  log = await observe(function (L) { return returnAcrossLoop(L); });
  assert.compareArray(
    log,
    ['body', 'disp', 'sync-end', 'w1', 'w2', 'resolved:ret', 'w3', 'w4', 'w5', 'w6'],
    'a return unwinding the loop suspends at the resource disposal Await'
  );

  // (c1) an unlabeled break with no intervening await reaches the inline
  // `Completion::Break` fast path rather than the `LoopControl` terminator.
  async function breakNoAwait(L) {
    for (await using a of [resource(L, '')]) { L('body'); break; }
    return 'after-break';
  }
  log = await observe(function (L) { return breakNoAwait(L); });
  assert.compareArray(
    log,
    ['body', 'disp', 'sync-end', 'w1', 'w2', 'resolved:after-break', 'w3', 'w4', 'w5', 'w6'],
    'an unlabeled break with no preceding await suspends at the resource disposal Await'
  );

  // (c2) a labeled break reached from a body that itself awaits first,
  // crossing two nested `await using` loops, reaches the `LoopControl`
  // terminator and `route_loop_control!`'s `unwind_for_of!` call.
  async function breakAfterAwaitAcrossNested(L) {
    outer: for (await using a of [resource(L, 'Outer')]) {
      for (await using b of [resource(L, 'Inner')]) {
        await null;
        L('body');
        break outer;
      }
    }
    return 'after-nested-break';
  }
  log = await observe(function (L) { return breakAfterAwaitAcrossNested(L); });
  assert.compareArray(
    log,
    [
      'sync-end', 'w1', 'body', 'dispInner', 'w2', 'dispOuter', 'w3', 'w4',
      'resolved:after-nested-break', 'w5', 'w6'
    ],
    'a labeled break crossing two nested loops suspends at each loop\'s disposal Await in turn'
  );

  // (e) a return inside an inner `await using`-bound loop whose async
  // disposer rejects, with a try/catch between the inner and outer loop
  // (the outer loop is not `await using`-bound). The return becomes a
  // throw mid-unwind; it must land at that catch, not propagate to or past
  // the outer loop, and the outer loop must still be open afterward — the
  // guard for `unwind_for_of!`'s per-level handler-boundary re-check
  // running across a suspension.
  var outerReturnCount = 0;
  function trackedOuter(values) {
    var index = 0;
    return {
      [Symbol.iterator]: function () { return this; },
      next: function () {
        return index < values.length
          ? { value: values[index++], done: false }
          : { value: undefined, done: true };
      },
      return: function () { outerReturnCount += 1; return { done: true }; }
    };
  }
  async function returnDisposerRejectsCaughtBeforeOuterLoop(L) {
    for (const outerValue of trackedOuter([1, 2])) {
      try {
        if (outerValue === 1) {
          for (await using inner of [rejectingResource(L, '', 'disposer-error')]) {
            L('body');
            return 'unreachable';
          }
        }
      } catch (e) {
        L('caught:' + e.message);
      }
      L('after-catch:' + outerValue);
    }
    return 'end';
  }
  log = await observe(function (L) { return returnDisposerRejectsCaughtBeforeOuterLoop(L); });
  assert.compareArray(
    log,
    [
      'body', 'disp', 'sync-end', 'w1', 'caught:disposer-error',
      'after-catch:1', 'after-catch:2', 'w2', 'resolved:end', 'w3', 'w4', 'w5', 'w6'
    ],
    'a return whose disposer rejects mid-unwind is caught before the outer loop, which stays open'
  );
  assert.sameValue(outerReturnCount, 0, 'the outer loop iterator is never closed via return()');
});
