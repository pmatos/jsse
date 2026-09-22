// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  A `for (await using x of iterable)` head's per-iteration DisposeResources
  suspends an async generator at its disposal Await instead of draining the
  job queue inline, so the rest of the synchronous caller runs first.
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
  generator, so the rest of the synchronous caller runs first.
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
      function (e) { L('n' + index + '-rej:' + (e && e.message || e)); }
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
  var log = await observe(function (L) {
    return [(async function* () {
      for (await using a of [null]) { L('body'); }
      L('after');
    })().next()];
  });
  assert.compareArray(
    log,
    ['body', 'sync-end', 'w1', 'after', 'w2', 'n0:undefined:true', 'w3', 'w4', 'w5', 'w6'],
    'a resource with no dispose method still Awaits once, and suspends the generator at it'
  );

  log = await observe(function (L) {
    return [(async function* () {
      for (await using a of [resource(L, '')]) { L('body'); }
      L('after');
    })().next()];
  });
  assert.compareArray(
    log,
    ['body', 'disp', 'sync-end', 'w1', 'after', 'w2', 'n0:undefined:true', 'w3', 'w4', 'w5', 'w6'],
    'an async disposer suspends the generator at its Await'
  );

  log = await observe(function (L) {
    return [(async function* () {
      for (await using a of [resource(L, 1), resource(L, 2)]) { L('body'); }
      L('after');
    })().next()];
  });
  assert.compareArray(
    log,
    ['body', 'disp1', 'sync-end', 'w1', 'body', 'disp2', 'w2', 'after', 'w3', 'n0:undefined:true', 'w4', 'w5', 'w6'],
    'each iteration disposes its own resource before the next iteration runs'
  );

  log = await observe(function (L) {
    return [(async function* () {
      for (await using a of [null]) { L('body'); await 0; }
      L('after');
    })().next()];
  });
  assert.compareArray(
    log,
    ['body', 'sync-end', 'w1', 'w2', 'after', 'w3', 'n0:undefined:true', 'w4', 'w5', 'w6'],
    'an await in the body ticks independently of the head disposal'
  );

  log = await observe(function (L) {
    return [(async function* () {
      for (
        await using a of [
          { [Symbol.asyncDispose]() { L('disp'); return Promise.reject(new Error('e2')); } }
        ]
      ) { L('body'); }
      L('after');
    })().next()];
  });
  assert.compareArray(
    log,
    ['body', 'disp', 'sync-end', 'w1', 'w2', 'n0-rej:e2', 'w3', 'w4', 'w5', 'w6'],
    'a rejecting disposer promise suspends the generator, whose request then rejects'
  );

  log = await observe(function (L) {
    var g = (async function* () {
      for (await using a of [resource(L, 1), resource(L, 2)]) { L('body'); yield 'y'; }
      L('after');
      return 'end';
    })();
    return [g.next(), g.next(), g.next()];
  });
  assert.compareArray(
    log,
    [
      'body', 'sync-end', 'w1', 'disp1', 'w2', 'n0:y:false',
      'body', 'w3', 'disp2', 'w4', 'n1:y:false',
      'after', 'w5', 'w6', 'n2:end:true'
    ],
    'queued requests stay behind the disposing request; a yield in the body does not change the alignment'
  );

  log = await observe(function (L) {
    var iterator = {
      i: 0,
      [Symbol.iterator]() { return this; },
      next() {
        return this.i++ < 3
          ? { value: { [Symbol.asyncDispose]() { L('disp'); return Promise.reject(new Error('e3')); } }, done: false }
          : { done: true };
      },
      return() { L('iter-return'); return {}; }
    };
    var g = (async function* () {
      try {
        for (await using a of iterator) { L('body'); yield 'y'; }
      } catch (e) {
        L('caught-' + e.message);
      }
      L('after');
      return 'end';
    })();
    return [g.next(), g.next(), g.next()];
  });
  assert.compareArray(
    log,
    [
      'body', 'sync-end', 'w1', 'disp', 'w2', 'n0:y:false',
      'iter-return', 'caught-e3', 'after', 'w3', 'w4', 'n1:end:true', 'n2:undefined:true', 'w5', 'w6'
    ],
    'a rejecting disposer ends the loop: the iterator closes once and the generator catch sees the error'
  );

  log = await observe(function (L) {
    var g = (async function* () {
      for (await using a of [resource(L, 1)]) { L('body'); yield 'y'; }
      L('after');
      return 'end';
    })();
    return [g.next(), g.return('R'), g.next()];
  });
  assert.compareArray(
    log,
    ['body', 'sync-end', 'w1', 'w2', 'n0:y:false', 'disp1', 'w3', 'w4', 'n1:R:true', 'n2:undefined:true', 'w5', 'w6'],
    'a return at the yield disposes the iteration resource before the request settles'
  );
});
