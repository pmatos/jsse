// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-generator-function-definitions-runtime-semantics-evaluation
description: >
  When a `.throw()` request is in flight during `yield*` delegation and the
  delegate has no `throw` method but a `return` method that succeeds
  synchronously with a plain (non-thenable) object, `AsyncIteratorClose`
  still `Await`s that result before the "no throw method" `TypeError`
  forms -- `Await` of a non-thenable value still costs a full microtask
  turn, so a marker microtask already queued before the `.throw()` call
  fires before the body's own `finally` sees the TypeError.
info: |
  AsyncIteratorClose ( iteratorRecord, completion )

  4.d. If innerResult is a normal completion, then
    i. Set innerResult to Completion(Await(innerResult.[[Value]])).
  7. If innerResult.[[Value]] is not an Object, throw a TypeError exception.
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
      return { value: v, done: true };
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
  Promise.resolve().then(function () { log.push('marker'); });
  var p = it.throw(new Error('injected'));

  var error;
  try {
    await p;
  } catch (e) {
    error = e;
  }
  log.push('caught:' + (error.constructor === TypeError));

  assert.compareArray(
    log,
    ['marker', 'finally', 'caught:true'],
    'the marker microtask, already queued before .throw() was even ' +
    'called, still fires before the body\'s finally: the Await of the ' +
    'synchronous non-thenable return() result was not skipped'
  );
  assert.sameValue(returnCalls, 1, 'return() is called exactly once');
});
