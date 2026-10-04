// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-generator-function-definitions-runtime-semantics-evaluation
description: >
  When a `.throw()` request is in flight during `yield*` delegation and the
  delegate has no `throw` method, `AsyncIteratorClose`'s own
  `GetMethod(iterator, "return")` can itself fail -- a throwing accessor, or
  a non-callable (but non-nullish) property value, which `GetMethod` must
  reject with its own `TypeError` -- and either failure overrides the "no
  throw method" `TypeError` entirely, delivered into the body exactly once.
info: |
  YieldExpression : yield * AssignmentExpression

  8.b.iii.3. If generatorKind is ~async~, perform ? AsyncIteratorClose(iteratorRecord, closeCompletion).

  AsyncIteratorClose ( iteratorRecord, completion )

  3. Let innerResult be Completion(GetMethod(iterator, "return")).

  GetMethod ( V, P )

  3. If func is either undefined or null, return undefined.
  4. If IsCallable(func) is false, throw a TypeError exception.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration]
---*/

asyncTest(async function () {
  // A throwing `return` accessor read during AsyncIteratorClose's own
  // GetMethod(iterator, "return") overrides the "no throw method" error.
  {
    var returnAccessorError = new Error('return-accessor-throws');
    var returnGets = 0;
    var delegate = {
      [Symbol.asyncIterator]() { return this; },
      next() { return Promise.resolve({ value: 'i1', done: false }); },
      get return() {
        returnGets++;
        throw returnAccessorError;
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
    assert.sameValue(error, returnAccessorError, 'the poisoned return accessor overrides the TypeError');
    assert.sameValue(returnGets, 1, 'the return property is read exactly once');
  }

  // A non-callable, non-nullish `return` property: GetMethod's own
  // IsCallable check throws TypeError, distinct from the "no throw
  // method" TypeError that would otherwise follow.
  {
    var delegate = {
      [Symbol.asyncIterator]() { return this; },
      next() { return Promise.resolve({ value: 'i1', done: false }); },
      return: 42
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
    assert.sameValue(error.constructor, TypeError, 'a non-callable return property is a TypeError, not "no method"');
  }
});
