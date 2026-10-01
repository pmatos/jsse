// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-generator-function-definitions-runtime-semantics-evaluation
description: >
  When a `.throw()` request is in flight during `yield*` delegation and the
  delegate has no `return` method either, `AsyncIteratorClose` has nothing
  to close (`GetMethod` found `undefined`) and the "no throw method"
  `TypeError` is delivered as the abrupt completion of the `YieldExpression`
  itself -- caught by the body's own `try`/`catch`/`finally` -- in the same
  microtask turn as the `.throw()` call, not as a directly rejected request
  promise and not after any extra suspension.
info: |
  YieldExpression : yield * AssignmentExpression

  8.b.iii. Else,
    1. Let receivedValue be received.[[Value]].
    2. Let closeCompletion be NormalCompletion(~empty~).
    3. If generatorKind is ~async~, perform ? AsyncIteratorClose(iteratorRecord, closeCompletion).
    4. Else, perform ? IteratorClose(iteratorRecord, closeCompletion).
    5. NOTE: The next step throws a TypeError to indicate that there was a
       yield* protocol violation: iterator does not have a throw method.
    6. Throw a TypeError exception.

  AsyncIteratorClose ( iteratorRecord, completion )

  4. Let innerResult be Completion(GetMethod(iterator, "return")).
  ... b. If return is undefined, return ? completion.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration]
---*/

asyncTest(async function () {
  var log = [];
  var calls = { next: 0 };
  var delegate = {
    [Symbol.asyncIterator]() { return this; },
    next() {
      calls.next++;
      return Promise.resolve({ value: 'i1', done: false });
    }
  };
  var it = (async function* () {
    try {
      yield* delegate;
    } catch (e) {
      log.push('caught:' + (e.constructor === TypeError));
    } finally {
      log.push('finally');
    }
  })();

  var first = await it.next();
  log.push('first:' + first.value);

  var p = it.throw(new Error('injected'));
  Promise.resolve().then(function () { log.push('marker'); });
  var r = await p;
  log.push('done:' + r.done);

  assert.compareArray(
    log,
    ['first:i1', 'caught:true', 'finally', 'marker', 'done:true'],
    'the delegate has no return method either, so AsyncIteratorClose owes ' +
    'no Await: the TypeError lands in the body\'s own catch/finally before ' +
    'the queued marker microtask, not as a directly settled request'
  );
  assert.sameValue(calls.next, 1, 'the delegate is never asked to advance again');
});
