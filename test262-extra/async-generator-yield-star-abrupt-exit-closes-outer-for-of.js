// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-generator-function-definitions-runtime-semantics-evaluation
description: >
  A `yield*` delegation that ends abruptly (here, a rejected inner `next()`
  result) with no enclosing `try` at all still throws into the generator
  body like any other abrupt completion of an expression -- so an enclosing
  `for-of` the `yield*` sits inside closes its own iterator (and disposes
  its per-iteration `await using` binding) on the way out, exactly as it
  would for any other throw originating in the loop body.
info: |
  YieldExpression : yield * AssignmentExpression

  8.a.ii. Let innerResult be ? Await(innerResult).

  The `?` is a ReturnIfAbrupt of the YieldExpression's own evaluation, so a
  rejected Await(innerResult) throws into the body exactly like any other
  abrupt completion of an expression -- including unwinding an enclosing
  `for-of` statement, whose own IteratorClose (and `await using` head
  disposal) must still run.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management, async-iteration]
---*/

asyncTest(async function () {
  var log = [];
  var returnCalls = 0;
  var outerIter = {
    [Symbol.iterator]() { return this; },
    next() {
      log.push('outer-next');
      return {
        value: { [Symbol.asyncDispose]() { log.push('x-disposed'); } },
        done: false
      };
    },
    return(v) {
      returnCalls++;
      log.push('outer-return');
      return { value: v, done: true };
    }
  };
  var innerDelegate = {
    [Symbol.asyncIterator]() { return this; },
    next() { return Promise.reject(new Error('boom')); }
  };

  var it = (async function* () {
    for (await using x of outerIter) {
      yield* innerDelegate;
    }
  })();

  var error;
  try {
    await it.next();
  } catch (e) {
    error = e;
  }

  assert.sameValue(error.message, 'boom', 'the rejected inner result throws into the body');
  assert.sameValue(returnCalls, 1, 'the outer for-of closes its own iterator exactly once');
  assert.compareArray(
    log,
    ['outer-next', 'x-disposed', 'outer-return'],
    'the per-iteration await using binding is disposed before the outer iterator is closed'
  );
});
