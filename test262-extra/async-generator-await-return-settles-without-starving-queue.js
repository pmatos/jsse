// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgeneratorawaitreturn
description: >
  AsyncGeneratorAwaitReturn's `PromiseResolve(value).then(...)` is scheduled
  like any other Await, through the ordinary microtask queue: it does not
  drain the whole queue inline, so a request queued behind a `.return()`
  settles only after that `.return()` settles (AsyncGeneratorDrainQueue runs
  from inside AsyncGeneratorAwaitReturn's own reaction, not before it), and
  unrelated pending microtasks are not forced to run to completion first.
info: |
  AsyncGeneratorAwaitReturn ( generator )

  1. Assert: generator.[[AsyncGeneratorState]] is draining-queue.
  ...
  7. Let promiseCompletion be Completion(PromiseResolve(%Promise%, completion.[[Value]])).
  ...
  12. Let fulfilledClosure be a new Abstract Closure ... performs the following steps when called:
    ...
    3. Perform AsyncGeneratorDrainQueue(generator).
  ...
  17. Perform PerformPromiseThen(promise, onFulfilled, onRejected).
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration]
---*/

function drainTicks(n) {
  var p = Promise.resolve();
  for (var i = 0; i < n; i++) { p = p.then(function () {}); }
  return p;
}

asyncTest(async function () {
  // A `.return()` reaching AsyncGeneratorAwaitReturn (no `finally` runs, no
  // resources to dispose: the generator has never started) settles on its
  // own, ahead of a `.next()` queued behind it, instead of the queue
  // advancing before the return's own Await settles.
  var log = [];
  var L = function (entry) { log.push(entry); };
  var it = (async function* () { yield 1; })();
  var ret = it.return('R');
  var nxt = it.next();
  ret.then(function (r) { L('ret:' + r.value + ':' + r.done); });
  nxt.then(function (r) { L('next:' + r.value + ':' + r.done); });
  await drainTicks(10);
  assert.compareArray(
    log,
    ['ret:R:true', 'next:undefined:true'],
    'the return settles before the request queued behind it'
  );
});

asyncTest(async function () {
  // The same ordering for a `.return()` that completes a `yield*` delegation
  // whose inner iterator has no `return` method (AsyncGeneratorAwaitReturn is
  // reached through GeneratorYield * 's own no-method arm, §15.5.5 step 8.c.ii).
  var log = [];
  var L = function (entry) { log.push(entry); };
  var noReturnMethod = {
    [Symbol.asyncIterator]() { return this; },
    next() { return { value: 1, done: false }; }
  };
  var it = (async function* () { yield* noReturnMethod; })();
  await it.next();
  var ret = it.return('D');
  var nxt = it.next();
  ret.then(function (r) { L('ret:' + r.value + ':' + r.done); });
  nxt.then(function (r) { L('next:' + r.value + ':' + r.done); });
  await drainTicks(10);
  assert.compareArray(
    log,
    ['ret:D:true', 'next:undefined:true'],
    'the delegated return settles before the request queued behind it'
  );
});

asyncTest(async function () {
  // The `.return()` settles within a small, bounded number of ticks even
  // while unrelated microtasks keep being scheduled: it must not depend on
  // the rest of the program's microtask activity running dry first.
  var log = [];
  var L = function (entry) { log.push(entry); };
  var it = (async function* () { yield 1; })();
  await it.next();
  var settled = false;
  var ret = it.return('S');
  ret.then(function () { settled = true; });
  var spins = 0;
  await new Promise(function (resolve) {
    (function spin() {
      spins++;
      if (settled || spins > 2000) { resolve(); return; }
      Promise.resolve().then(spin);
    })();
  });
  assert(settled, 'the return settled without waiting for unrelated recurring microtasks (spun ' + spins + ' times)');
});
