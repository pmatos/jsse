// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-generator-function-definitions-runtime-semantics-evaluation
description: >
  When a `yield*` delegation's second step fails -- the inner `next()` call's
  Await rejects, the awaited inner result is not an object, its `done`
  getter throws, or its `value` getter throws -- the failure is the abrupt
  completion of the `YieldExpression` itself. It must propagate through an
  enclosing `finally` (and, had there been one, a `catch`) exactly like any
  other abrupt completion of an expression, instead of settling the
  `.next()` request directly and skipping the generator body entirely.
info: |
  YieldExpression : yield * AssignmentExpression

  8.a.ii. Let innerResult be ? Await(innerResult).
  8.a.iii. If innerResult is not an Object, throw a TypeError exception.
  8.a.iv. Let done be ? IteratorComplete(innerResult).
  8.a.v. If done is true, [...]
  8.a.vi. Let received be NormalCompletion(? IteratorValue(innerResult)).

  Each `?` is a ReturnIfAbrupt of the YieldExpression's own evaluation, so
  the resulting throw completion is routed like any other: through the
  nearest enclosing `catch`/`finally`, not settled on the request promise
  directly.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management, async-iteration]
---*/

function makeDelegate(afterFirst) {
  var calls = { next: 0, throw: 0, return: 0 };
  var delegate = {
    [Symbol.asyncIterator]() { return this; },
    next() {
      calls.next++;
      if (calls.next === 1) {
        return Promise.resolve({ value: 'i1', done: false });
      }
      return afterFirst();
    },
    throw(e) { calls.throw++; return Promise.reject(e); },
    return(v) { calls.return++; return Promise.resolve({ value: v, done: true }); }
  };
  return { delegate: delegate, calls: calls };
}

async function run(afterFirst) {
  var log = [];
  var made = makeDelegate(afterFirst);
  var it = (async function* () {
    try {
      yield* made.delegate;
    } finally {
      log.push('f');
    }
  })();

  var first = await it.next();
  log.push('first:' + first.value);

  var error;
  try {
    await it.next();
    log.push('no-throw');
  } catch (e) {
    error = e;
  }

  return { log: log, error: error, calls: made.calls };
}

asyncTest(async function () {
  // 8.a.ii: Await(innerResult) rejects.
  var rejectError = new Error('rejected-inner-result');
  var r = await run(function () { return Promise.reject(rejectError); });
  assert.sameValue(r.error, rejectError, 'a rejected inner result throws into the body');
  assert.compareArray(
    r.log,
    ['first:i1', 'f'],
    'finally runs before the request rejects (rejected inner result)'
  );
  assert.sameValue(r.calls.next, 2, 'next() is called exactly twice');
  assert.sameValue(r.calls.throw, 0, 'the delegate\'s throw() is never called');
  assert.sameValue(r.calls.return, 0, 'the delegate\'s return() is never called');

  // 8.a.iii: innerResult is not an Object.
  r = await run(function () { return Promise.resolve(42); });
  assert.sameValue(r.error.constructor, TypeError, 'a non-object inner result throws a TypeError');
  assert.compareArray(
    r.log,
    ['first:i1', 'f'],
    'finally runs before the request rejects (non-object inner result)'
  );
  assert.sameValue(r.calls.next, 2, 'next() is called exactly twice');
  assert.sameValue(r.calls.throw, 0, 'the delegate\'s throw() is never called');
  assert.sameValue(r.calls.return, 0, 'the delegate\'s return() is never called');

  // 8.a.iv: IteratorComplete(innerResult) -- the `done` getter throws.
  var doneError = new Error('done-getter-throws');
  r = await run(function () {
    return Promise.resolve({ get done() { throw doneError; }, value: 'x' });
  });
  assert.sameValue(r.error, doneError, 'a throwing done getter throws into the body');
  assert.compareArray(
    r.log,
    ['first:i1', 'f'],
    'finally runs before the request rejects (done getter throws)'
  );
  assert.sameValue(r.calls.next, 2, 'next() is called exactly twice');
  assert.sameValue(r.calls.throw, 0, 'the delegate\'s throw() is never called');
  assert.sameValue(r.calls.return, 0, 'the delegate\'s return() is never called');

  // 8.a.vi: IteratorValue(innerResult) -- the `value` getter throws. This is
  // the arm that previously special-cased "has an enclosing catch"; with no
  // catch at all, today's code rejects the request directly and never runs
  // the enclosing finally.
  var valueError = new Error('value-getter-throws');
  r = await run(function () {
    return Promise.resolve({ done: false, get value() { throw valueError; } });
  });
  assert.sameValue(r.error, valueError, 'a throwing value getter throws into the body');
  assert.compareArray(
    r.log,
    ['first:i1', 'f'],
    'finally runs before the request rejects (value getter throws, no catch)'
  );
  assert.sameValue(r.calls.next, 2, 'next() is called exactly twice');
  assert.sameValue(r.calls.throw, 0, 'the delegate\'s throw() is never called');
  assert.sameValue(r.calls.return, 0, 'the delegate\'s return() is never called');
});
