// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgeneratorvalidate
description: >
  %AsyncGeneratorPrototype%.next/return/throw return a promise rejected with a
  TypeError, and never throw synchronously, when the receiver is an object that
  lacks the async generator internal slots (a sync generator object, an object
  inheriting from an async generator, a proxy of one); a real async generator
  still works through the same three entry points.
info: |
  %AsyncGeneratorPrototype%.next ( value )

  1. Let generator be the this value.
  2. Let promiseCapability be ! NewPromiseCapability(%Promise%).
  3. Let result be Completion(AsyncGeneratorValidate(generator, empty)).
  4. IfAbruptRejectPromise(result, promiseCapability).

  AsyncGeneratorValidate ( generator, brand )

  1. Perform ? RequireInternalSlot(generator, [[AsyncGeneratorState]]).
  2. Perform ? RequireInternalSlot(generator, [[AsyncGeneratorQueue]]).
flags: [async]
includes: [asyncHelpers.js]
features: [async-iteration]
---*/

var AsyncGeneratorPrototype = Object.getPrototypeOf(async function* () {}).prototype;

var syncGen = (function* () { yield 1; })();
var realAsyncGen = (async function* () { yield 1; })();
var receivers = {
  'sync generator object': syncGen,
  'object inheriting from an async generator': Object.create(realAsyncGen),
  'proxy of an async generator': new Proxy(realAsyncGen, {})
};

var methods = ['next', 'return', 'throw'];

asyncTest(async function () {
  for (var name of Object.keys(receivers)) {
    for (var method of methods) {
      var promise;
      try {
        promise = AsyncGeneratorPrototype[method].call(receivers[name], 1);
      } catch (e) {
        throw new Test262Error(method + ' threw synchronously for ' + name + ': ' + e);
      }
      assert(promise instanceof Promise, method + ' returns a promise for ' + name);

      var caught;
      try {
        await promise;
      } catch (e) {
        caught = e;
      }
      assert.notSameValue(caught, undefined, method + ' rejects for ' + name);
      assert.sameValue(caught.constructor, TypeError, method + ' rejects with TypeError for ' + name);
    }
  }

  var nextGen = (async function* () { yield 1; })();
  var nextResult = await AsyncGeneratorPrototype.next.call(nextGen, 'ignored');
  assert.sameValue(nextResult.value, 1, 'next value');
  assert.sameValue(nextResult.done, false, 'next done');

  var returnGen = (async function* () { yield 1; })();
  var returnResult = await AsyncGeneratorPrototype.return.call(returnGen, 'r');
  assert.sameValue(returnResult.value, 'r', 'return value');
  assert.sameValue(returnResult.done, true, 'return done');

  var throwGen = (async function* () { yield 1; })();
  var thrown = new Test262Error('thrown');
  var throwCaught;
  try {
    await AsyncGeneratorPrototype.throw.call(throwGen, thrown);
  } catch (e) {
    throwCaught = e;
  }
  assert.sameValue(throwCaught, thrown, 'throw rejects with the thrown value');
});
