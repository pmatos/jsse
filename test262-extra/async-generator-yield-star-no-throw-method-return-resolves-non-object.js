// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-generator-function-definitions-runtime-semantics-evaluation
description: >
  When a `.throw()` request is in flight during `yield*` delegation and the
  delegate has no `throw` method but a `return` method whose result
  resolves (via `Await`) to a non-Object, `AsyncIteratorClose` itself
  raises a `TypeError` (its own step 7) rather than falling through to the
  "no throw method" `TypeError` -- and that TypeError is observed only
  after the Await actually settles, not synchronously off the `Call`.
info: |
  AsyncIteratorClose ( iteratorRecord, completion )

  4.d. If innerResult is a normal completion, then
    i. Set innerResult to Completion(Await(innerResult.[[Value]])).
  7. If innerResult.[[Value]] is not an Object, throw a TypeError exception.
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
      return Promise.resolve(42);
    }
  };
  var it = (async function* () {
    yield* delegate;
  })();

  await it.next();
  var p = it.throw(new Error('injected'));
  Promise.resolve().then(function () { log.push('marker'); });

  var error;
  try {
    await p;
  } catch (e) {
    error = e;
  }
  log.push('caught');

  assert.compareArray(
    log,
    ['marker', 'caught'],
    'the TypeError from AsyncIteratorClose step 7 is observed only after ' +
    'the Await of the non-object result settles'
  );
  assert.sameValue(error.constructor, TypeError, 'a non-object return() result is its own TypeError');
  assert.sameValue(returnCalls, 1, 'return() is called exactly once');
});
