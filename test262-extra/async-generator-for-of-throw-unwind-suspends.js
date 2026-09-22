// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  When a `throw` inside `for (await using x of iterable) { ... }` unwinds the
  loop, closing its per-iteration `await using` resource suspends the async
  generator at the disposal's Await instead of draining the job queue inline,
  so the rest of the synchronous caller runs first.
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

  A witness chain of promise reactions is started before the request
  promises get their own reactions, so the position of each marker pins the
  number of ticks each disposal consumed. The disposal Await suspends the
  generator (rather than draining the job queue inline), so the rest of the
  synchronous caller runs first.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management, async-iteration]
---*/

function witnessChain(L, count) {
  var p = Promise.resolve();
  for (var i = 1; i <= count; i++) {
    (function (n) { p = p.then(function () { L('w' + n); }); })(i);
  }
  return p;
}

async function observe(shape) {
  var log = [];
  var L = function (entry) { log.push(entry); };
  witnessChain(L, 6);
  var requests = shape(L);
  requests.forEach(function (request, index) {
    request.then(
      function (r) { L('n' + index + ':' + r.value + ':' + r.done); },
      function (e) {
        var detail = (e && e.constructor.name === 'SuppressedError')
          ? e.constructor.name + '(' + e.error.message + ',' + e.suppressed.message + ')'
          : (e && e.message || e);
        L('n' + index + '-rej:' + detail);
      }
    );
  });
  L('sync-end');
  var drain = Promise.resolve();
  for (var i = 0; i < 20; i++) { drain = drain.then(function () {}); }
  await drain;
  return log;
}

function resource(L, name) {
  return { [Symbol.asyncDispose]() { L('disp' + name); } };
}

asyncTest(async function () {
  // A throw inside the loop body unwinds it: the resource's async disposer
  // must suspend the generator at its Await, not run inline before
  // `sync-end` and the witness chain fire.
  var log = await observe(function (L) {
    return [(async function* () {
      for (await using a of [resource(L, '')]) { L('body'); throw new Error('boom'); }
    })().next()];
  });
  assert.compareArray(
    log,
    ['body', 'disp', 'sync-end', 'w1', 'w2', 'n0-rej:boom', 'w3', 'w4', 'w5', 'w6'],
    'a throw unwinding the loop suspends the generator at the resource disposal Await'
  );

  // A throw crossing two nested for-of loops closes the inner resource
  // first, each closing suspending independently.
  log = await observe(function (L) {
    return [(async function* () {
      for (await using a of [resource(L, 'Outer')]) {
        for (await using b of [resource(L, 'Inner')]) {
          L('body');
          throw new Error('nested');
        }
      }
    })().next()];
  });
  assert.compareArray(
    log,
    ['body', 'dispInner', 'sync-end', 'w1', 'dispOuter', 'w2', 'w3', 'n0-rej:nested', 'w4', 'w5', 'w6'],
    'a throw unwinding nested for-of loops suspends at each loop\'s disposal Await in turn'
  );

  // A throw crossing a for-of loop whose disposer itself throws: the
  // disposer's error replaces the original, still suspending at the Await.
  log = await observe(function (L) {
    return [(async function* () {
      for (
        await using a of [
          { [Symbol.asyncDispose]() { L('disp'); return Promise.reject(new Error('disposer-error')); } }
        ]
      ) {
        L('body');
        throw new Error('original');
      }
    })().next()];
  });
  assert.compareArray(
    log,
    ['body', 'disp', 'sync-end', 'w1', 'w2', 'n0-rej:SuppressedError(disposer-error,original)', 'w3', 'w4', 'w5', 'w6'],
    'a disposer that itself throws while unwinding still suspends, chaining its error onto the original'
  );

  // A throw from inside a try/catch surrounding the for-of loop: the
  // catch handler still runs, only after the disposal suspends and resumes.
  log = await observe(function (L) {
    return [(async function* () {
      try {
        for (await using a of [resource(L, '')]) { L('body'); throw new Error('caught-me'); }
      } catch (e) {
        L('caught-' + e.message);
      }
      L('after');
      return 'end';
    })().next()];
  });
  assert.compareArray(
    log,
    ['body', 'disp', 'sync-end', 'w1', 'caught-caught-me', 'after', 'w2', 'w3', 'n0:end:true', 'w4', 'w5', 'w6'],
    'a throw caught by an enclosing try still suspends the loop\'s disposal before the catch runs'
  );
});
