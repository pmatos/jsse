// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-generator-function-definitions-runtime-semantics-evaluation
description: >
  When a `.throw()` request is in flight during `yield*` delegation and the
  delegate has no `throw` method, if the delegate's `return` method itself
  throws synchronously (before any `Await`), that error overrides the "no
  throw method" `TypeError`, delivered into the body exactly once. The call
  passes no arguments, unlike the delegated-return step's own `Call`.
info: |
  AsyncIteratorClose ( iteratorRecord, completion )

  4. If innerResult is a normal completion, then
    c. If IsCallable(return) is true, then
      i. Set innerResult to Completion(Call(return, iterator)).
  6. If innerResult is a throw completion, return ? innerResult.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration]
---*/

asyncTest(async function () {
  var returnCallError = new Error('return-call-throws');
  var returnCalls = 0;
  var receivedArgs;
  var delegate = {
    [Symbol.asyncIterator]() { return this; },
    next() { return Promise.resolve({ value: 'i1', done: false }); },
    return() {
      returnCalls++;
      receivedArgs = arguments.length;
      throw returnCallError;
    }
  };
  var it = (async function* () {
    yield* delegate;
  })();
  await it.next();

  var error;
  try {
    await it.throw(new Error('injected'));
  } catch (e) {
    error = e;
  }

  assert.sameValue(error, returnCallError, 'the synchronous return() throw overrides the TypeError');
  assert.sameValue(returnCalls, 1, 'return() is called exactly once');
  assert.sameValue(receivedArgs, 0, 'AsyncIteratorClose calls return() with no arguments');
});
