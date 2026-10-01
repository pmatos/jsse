// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-generator-function-definitions-runtime-semantics-evaluation
description: >
  When a `.throw()` request is in flight during `yield*` delegation and the
  delegate has no `throw` method but a `return` method whose result is a
  promise that rejects, that rejection reason takes priority over the "no
  throw method" `TypeError` and is delivered into the body's own
  `try`/`catch` -- this is the exact scenario #780 was filed over: a
  rejecting async `.return()` must not have its rejection silently dropped.
info: |
  AsyncIteratorClose ( iteratorRecord, completion )

  4.d. If innerResult is a normal completion, then
    i. Set innerResult to Completion(Await(innerResult.[[Value]])).
  6. If innerResult is a throw completion, return ? innerResult.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration]
---*/

asyncTest(async function () {
  var returnCalls = 0;
  var returnRejection = new Error('return-rejects');
  var delegate = {
    [Symbol.asyncIterator]() { return this; },
    next() { return Promise.resolve({ value: 'i1', done: false }); },
    return(v) {
      returnCalls++;
      return Promise.reject(returnRejection);
    }
  };
  var observedInBody;
  var it = (async function* () {
    try {
      yield* delegate;
    } catch (e) {
      observedInBody = e;
      throw e;
    }
  })();

  await it.next();

  var error;
  try {
    await it.throw(new Error('injected'));
  } catch (e) {
    error = e;
  }

  assert.sameValue(error, returnRejection, 'the return() rejection overrides the TypeError, observed by the caller');
  assert.sameValue(observedInBody, returnRejection, 'the body\'s own catch sees the rejection, not a TypeError');
  assert.sameValue(returnCalls, 1, 'return() is called exactly once');
});
