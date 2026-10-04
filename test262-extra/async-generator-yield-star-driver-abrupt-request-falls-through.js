// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-generator-function-definitions-runtime-semantics-evaluation
description: >
  A request that arrives while an async generator is already parked in a
  `yield*` delegation (a `.return()`/`.throw()`/`.next()` call made before
  the delegation's own in-flight step settles) can itself fail before any
  `Await`: `GetMethod(iterator, "return")`/`GetMethod(iterator, "throw")`
  throwing, or a synchronous `next()` call throwing. That failure is the
  abrupt completion of the `yield*` expression itself and must propagate
  through the body's enclosing `finally`, not settle the new request
  directly -- and, since the failure is not notice to the delegate, the
  delegate's own `next`/`throw` methods must not be called afterward.
info: |
  YieldExpression : yield * AssignmentExpression

  7.c.ii. Let return be ? GetMethod(iterator, "return").
  8.b.i. Let throw be ? GetMethod(iterator, "throw").
  8.a.i. Let innerResult be ? Call(nextMethod, iterator, « received.[[Value]] »).

  Each `?` is a ReturnIfAbrupt of the YieldExpression's own evaluation.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management, async-iteration]
---*/

async function run(makeDelegate) {
  var log = [];
  var made = makeDelegate();
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
    await made.trigger(it);
    log.push('no-throw');
  } catch (e) {
    error = e;
  }

  return { log: log, error: error, calls: made.calls };
}

asyncTest(async function () {
  // A poisoned `return` accessor read while a `.return()` call is in flight
  // during delegation (GetMethod(iterator, "return") step).
  var returnAccessorError = new Error('return-accessor-throws');
  var r = await run(function () {
    var calls = { next: 0, throw: 0 };
    var delegate = {
      [Symbol.asyncIterator]() { return this; },
      next() { calls.next++; return Promise.resolve({ value: 'i1', done: false }); },
      get throw() { calls.throw++; return undefined; },
      get return() { throw returnAccessorError; }
    };
    return {
      delegate: delegate,
      calls: calls,
      trigger: function (it) { return it.return('R'); }
    };
  });
  assert.sameValue(r.error, returnAccessorError, 'the poisoned return accessor throws into the body');
  assert.compareArray(
    r.log,
    ['first:i1', 'f'],
    'finally runs before the .return() request rejects'
  );
  assert.sameValue(r.calls.next, 1, 'the delegate\'s next() is not called again');
  assert.sameValue(r.calls.throw, 0, 'the delegate\'s throw accessor is never read');

  // A poisoned `throw` accessor read while a `.throw()` call is in flight
  // during delegation (GetMethod(iterator, "throw") step).
  var throwAccessorError = new Error('throw-accessor-throws');
  r = await run(function () {
    var calls = { next: 0, return: 0 };
    var delegate = {
      [Symbol.asyncIterator]() { return this; },
      next() { calls.next++; return Promise.resolve({ value: 'i1', done: false }); },
      get return() { calls.return++; return undefined; },
      get throw() { throw throwAccessorError; }
    };
    return {
      delegate: delegate,
      calls: calls,
      trigger: function (it) { return it.throw(new Error('injected')); }
    };
  });
  assert.sameValue(r.error, throwAccessorError, 'the poisoned throw accessor throws into the body');
  assert.compareArray(
    r.log,
    ['first:i1', 'f'],
    'finally runs before the .throw() request rejects'
  );
  assert.sameValue(r.calls.next, 1, 'the delegate\'s next() is not called again');
  assert.sameValue(r.calls.return, 0, 'the delegate\'s return accessor is never read');

  // A synchronous throw out of next() while a plain .next() call is in
  // flight during delegation (step 8.a.i's own Call failing, before any
  // Await is even reached).
  var nextThrowError = new Error('next-throws-synchronously');
  r = await run(function () {
    var calls = { next: 0, throw: 0, return: 0 };
    var delegate = {
      [Symbol.asyncIterator]() { return this; },
      next() {
        calls.next++;
        if (calls.next === 1) {
          return Promise.resolve({ value: 'i1', done: false });
        }
        throw nextThrowError;
      },
      throw(e) { calls.throw++; return Promise.reject(e); },
      return(v) { calls.return++; return Promise.resolve({ value: v, done: true }); }
    };
    return {
      delegate: delegate,
      calls: calls,
      trigger: function (it) { return it.next(); }
    };
  });
  assert.sameValue(r.error, nextThrowError, 'the synchronous next() throw propagates into the body');
  assert.compareArray(
    r.log,
    ['first:i1', 'f'],
    'finally runs before the second .next() request rejects'
  );
  assert.sameValue(r.calls.next, 2, 'next() is called exactly twice (no extra retry)');
  assert.sameValue(r.calls.throw, 0, 'the delegate\'s throw() is never called');
  assert.sameValue(r.calls.return, 0, 'the delegate\'s return() is never called');
});
