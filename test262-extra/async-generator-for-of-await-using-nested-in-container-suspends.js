// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  A `for (await using x of iterable)` head nested inside a container (`if`,
  `try`/`finally`, or a loop nested in another loop) with no other
  `await`/`yield` of its own is still lowered into the generator/async
  function state machine, so its per-iteration disposal suspends at the
  Await instead of draining the job queue inline. Regression test for issue
  #733 item 4 (already fixed by #737's recursive stmt_contains_for_of_head).
info: |
  ForIn/OfBodyEvaluation ( lhs, stmt, iteratorRecord, iterationKind, lhsKind, labelSet [ , iteratorKind ] )

  A witness chain of promise reactions is started before the request
  promises get their own reactions, so the position of each marker pins the
  number of ticks each disposal consumed. The disposal Await suspends the
  driver, so the rest of the synchronous caller runs first.
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
      function (r) { L('n' + index + ':' + JSON.stringify(r)); },
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
  // Async generator: for-of-with-await-using nested in `if`.
  var log = await observe(function (L) {
    return [(async function* () {
      if (true) {
        for (await using a of [resource(L, '')]) { L('body'); }
      }
      L('after');
    })().next()];
  });
  assert.compareArray(
    log,
    ['body', 'disp', 'sync-end', 'w1', 'after', 'w2', 'n0:{\"done\":true}', 'w3', 'w4', 'w5', 'w6'],
    'async generator: for-of-with-await-using nested in `if` suspends at its disposal Await'
  );

  // Async generator: for-of-with-await-using nested in `try { } finally { }`.
  log = await observe(function (L) {
    return [(async function* () {
      try {
        for (await using a of [resource(L, '')]) { L('body'); }
      } finally {
        L('finally');
      }
      L('after');
    })().next()];
  });
  assert.compareArray(
    log,
    ['body', 'disp', 'sync-end', 'w1', 'finally', 'after', 'w2', 'n0:{\"done\":true}', 'w3', 'w4', 'w5', 'w6'],
    'async generator: for-of-with-await-using nested in try/finally suspends at its disposal Await'
  );

  // Async generator: for-of-with-await-using nested inside another loop's body.
  log = await observe(function (L) {
    return [(async function* () {
      for (var i = 0; i < 2; i++) {
        for (await using a of [resource(L, i)]) { L('body' + i); }
      }
      L('after');
    })().next()];
  });
  assert.compareArray(
    log,
    ['body0', 'disp0', 'sync-end', 'w1', 'body1', 'disp1', 'w2', 'after', 'w3', 'n0:{\"done\":true}', 'w4', 'w5', 'w6'],
    'async generator: for-of-with-await-using nested in another loop suspends at each iteration\'s disposal Await'
  );

  // Async function: the same three shapes must suspend identically.
  log = await observe(function (L) {
    return [(async function () {
      if (true) {
        for (await using a of [resource(L, '')]) { L('body'); }
      }
      L('after');
      return 'end';
    })()];
  });
  assert.compareArray(
    log,
    ['body', 'disp', 'sync-end', 'w1', 'after', 'w2', 'n0:\"end\"', 'w3', 'w4', 'w5', 'w6'],
    'async function: for-of-with-await-using nested in `if` suspends at its disposal Await'
  );

  log = await observe(function (L) {
    return [(async function () {
      try {
        for (await using a of [resource(L, '')]) { L('body'); }
      } finally {
        L('finally');
      }
      L('after');
      return 'end';
    })()];
  });
  assert.compareArray(
    log,
    ['body', 'disp', 'sync-end', 'w1', 'finally', 'after', 'w2', 'n0:\"end\"', 'w3', 'w4', 'w5', 'w6'],
    'async function: for-of-with-await-using nested in try/finally suspends at its disposal Await'
  );
});
