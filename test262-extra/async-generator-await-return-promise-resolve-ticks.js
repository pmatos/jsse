// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgeneratorawaitreturn
description: >
  AsyncGeneratorAwaitReturn runs PromiseResolve(%Promise%, value) and a single
  PerformPromiseThen: a native promise operand costs exactly one job before
  the request settles, and the call itself runs no job.
info: |
  AsyncGeneratorAwaitReturn ( generator )

  [...]
  7. Let promiseCompletion be Completion(PromiseResolve(%Promise%, completion.[[Value]])).
  [...]
  13. Perform PerformPromiseThen(promise, onFulfilled, onRejected).

  PromiseResolve returns a promise whose constructor is %Promise% unchanged, so
  no NewPromiseResolveThenableJob is queued for it.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration]
---*/

async function* g() {}

function ticks(log, n) {
  var p = Promise.resolve();
  for (var i = 1; i <= n; i++) {
    (function (i) { p = p.then(function () { log.push('w' + i); }); })(i);
  }
  return p;
}

asyncTest(async function () {
  var log = [];
  Promise.resolve().then(function () { log.push('job'); });
  var result = g().return(Promise.reject('E'));
  log.push('sync-end');
  var settled = result.then(function () { log.push('fulfilled'); }, function () { log.push('rejected'); });
  await Promise.all([settled, ticks(log, 4)]);
  assert.compareArray(
    log,
    ['sync-end', 'job', 'w1', 'rejected', 'w2', 'w3', 'w4'],
    'a rejected native promise operand rejects the request one job after the call'
  );

  log = [];
  result = g().return(Promise.resolve('V'));
  log.push('sync-end');
  settled = result.then(function (r) { log.push('fulfilled:' + r.value); });
  await Promise.all([settled, ticks(log, 3)]);
  assert.compareArray(
    log,
    ['sync-end', 'w1', 'fulfilled:V', 'w2', 'w3'],
    'a fulfilled native promise operand settles the request one job after the call'
  );

  log = [];
  result = g().return({ then: function (resolve) { resolve('T'); } });
  log.push('sync-end');
  settled = result.then(function (r) { log.push('fulfilled:' + r.value); });
  await Promise.all([settled, ticks(log, 4)]);
  assert.compareArray(
    log,
    ['sync-end', 'w1', 'w2', 'fulfilled:T', 'w3', 'w4'],
    'a thenable operand costs the NewPromiseResolveThenableJob plus the reaction'
  );
});
