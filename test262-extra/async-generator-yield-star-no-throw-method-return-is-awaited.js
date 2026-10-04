// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-generator-function-definitions-runtime-semantics-evaluation
description: >
  When a `.throw()` request is in flight during `yield*` delegation and the
  delegate has no `throw` method but a `return` method that itself returns
  a promise resolving only after several queued microtask ticks,
  `AsyncIteratorClose` genuinely parks the generator at that `Await`:
  intervening microtasks interleave before the "no throw method"
  `TypeError` settles into the body, and `return()` is called exactly once.
info: |
  AsyncIteratorClose ( iteratorRecord, completion )

  4.d. If innerResult is a normal completion, then
    i. Set innerResult to Completion(Await(innerResult.[[Value]])).
  8. Return ? completion.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration]
---*/

asyncTest(async function () {
  var log = [];
  var returnCalls = 0;
  var delegate = {
    [Symbol.asyncIterator]() { return this; },
    next() { return Promise.resolve({ value: 'i1', done: false }); },
    return(v) {
      returnCalls++;
      return new Promise(function (resolve) {
        var p = Promise.resolve();
        for (var i = 0; i < 3; i++) { p = p.then(function () {}); }
        p.then(function () { resolve({ value: v, done: true }); });
      });
    }
  };
  var it = (async function* () {
    try {
      yield* delegate;
    } finally {
      log.push('finally');
    }
  })();

  await it.next();
  var p = it.throw(new Error('injected'));

  var tickCount = 0;
  function countTick() {
    tickCount++;
    log.push('tick' + tickCount);
    if (tickCount < 6) Promise.resolve().then(countTick);
  }
  Promise.resolve().then(countTick);

  var error;
  try {
    await p;
  } catch (e) {
    error = e;
  }
  log.push('caught:' + (error.constructor === TypeError));

  assert.sameValue(returnCalls, 1, 'return() is called exactly once');
  assert.compareArray(
    log,
    ['tick1', 'tick2', 'tick3', 'tick4', 'finally', 'tick5', 'caught:true'],
    'four queued ticks interleave before the cross-tick Await settles and ' +
    'the body\'s finally runs, with the outer catch observing the ' +
    'TypeError only after a further tick'
  );
});
