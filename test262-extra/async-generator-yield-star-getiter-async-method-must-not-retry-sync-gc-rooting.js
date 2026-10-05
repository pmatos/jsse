// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-getmethod
description: >
  In an async generator's `yield* AssignmentExpression`, GetIterator's
  resolution of %Symbol.asyncIterator% must never fall back to
  %Symbol.iterator% after GetMethod(obj, %Symbol.asyncIterator%) itself
  produces an abrupt completion -- that fallback is only reachable when
  GetMethod *returns* undefined. The error value from such an abrupt
  completion must also survive a garbage collection triggered by any
  further call made while resolving the delegate's iterator.
info: |
  GetIterator ( obj, kind )

  1. If kind is async, then
    a. Let method be ? GetMethod(obj, %Symbol.asyncIterator%).
    b. If method is undefined, then
      i. Let syncMethod be ? GetMethod(obj, %Symbol.iterator%).
      ...

  GetMethod ( V, P )

  1. Let func be ? GetV(V, P).
  2. If func is either undefined or null, return undefined.
  3. If IsCallable(func) is false, throw a TypeError exception.
  4. Return func.

  Step 1.b of GetIterator is reached only when GetMethod(obj,
  %Symbol.asyncIterator%) returns undefined (GetMethod step 2); an abrupt
  completion from GetMethod step 1 or step 3 propagates directly out of
  GetIterator via the "?" on step 1.a and never reaches step 1.b.

  This engine settles that abrupt completion by rejecting the async
  generator's result promise directly, rather than routing it through any
  try/catch in the generator body -- so both scenarios below observe the
  error via the rejection of `next()`'s result promise, not via a JS-level
  catch.
flags: [async]
includes: [asyncHelpers.js]
features: [async-iteration, host-gc-required]
---*/

asyncTest(async function () {
  // Scenario A: %Symbol.asyncIterator% is present but not callable, so
  // GetMethod throws at its own step 3. The %Symbol.iterator% getter below
  // must never run -- any invocation signals the forbidden fallback.
  var getterCallsA = 0;
  var delegateA = {
    [Symbol.asyncIterator]: {},
    get [Symbol.iterator]() {
      getterCallsA++;
      $262.gc();
      // Allocation churn so a freed-but-unrooted error value is likely to
      // have its arena slot reused before the caller reads it back.
      for (var i = 0; i < 64; i++) {
        [{}, {}, {}];
      }
      return function () {
        return { next() { return { value: undefined, done: true }; } };
      };
    },
  };

  var itA = (async function* () {
    yield* delegateA;
  })();

  var errA;
  try {
    await itA.next();
    throw new Test262Error('scenario A: expected next() to reject');
  } catch (e) {
    errA = e;
  }
  assert.sameValue(
    getterCallsA,
    0,
    'scenario A: %Symbol.iterator% must never be consulted after a non-callable %Symbol.asyncIterator% throws'
  );
  assert.sameValue(
    errA.constructor,
    TypeError,
    'scenario A: the TypeError from GetMethod survives, not a corrupted/reused object'
  );

  // Scenario B: %Symbol.asyncIterator% is absent, so GetIterator's own
  // internal sync fallback consults %Symbol.iterator%, whose getter throws.
  // That getter must be invoked exactly once -- a buggy external retry
  // would invoke it a second time.
  var getterCallsB = 0;
  var sentinel = { tag: 'issue-828-sentinel' };
  var delegateB = {
    [Symbol.asyncIterator]: undefined,
    get [Symbol.iterator]() {
      getterCallsB++;
      $262.gc();
      for (var i = 0; i < 64; i++) {
        [{}, {}, {}];
      }
      throw sentinel;
    },
  };

  var itB = (async function* () {
    yield* delegateB;
  })();

  var errB;
  try {
    await itB.next();
    throw new Test262Error('scenario B: expected next() to reject');
  } catch (e) {
    errB = e;
  }
  assert.sameValue(
    getterCallsB,
    1,
    'scenario B: %Symbol.iterator% must be invoked exactly once, not retried after it already threw'
  );
  assert.sameValue(
    errB,
    sentinel,
    'scenario B: the original thrown value survives the gc() inside its own getter, not a corrupted/reused object'
  );
  assert.sameValue(errB.tag, 'issue-828-sentinel', 'scenario B: the sentinel tag is intact');
});
