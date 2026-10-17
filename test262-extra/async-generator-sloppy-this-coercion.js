// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-ordinarycallbindthis
description: >
  A sloppy-mode async generator function binds `this` like any other ordinary
  function: a nullish receiver becomes the global object and a primitive one
  its wrapper object, while a strict function sees the receiver unchanged.
  The binding is made when the generator function is called, and arrow
  functions inside the body see it too.
info: |
  OrdinaryCallBindThis ( F, calleeContext, thisArgument )

  1. Let thisMode be F.[[ThisMode]].
  2. If thisMode is lexical, return unused.
  3. Let calleeRealm be F.[[Realm]].
  4. Let localEnv be the LexicalEnvironment of calleeContext.
  5. If thisMode is strict, let thisValue be thisArgument.
  6. Else,
    a. If thisArgument is either undefined or null, then
      i. Let globalEnv be calleeRealm.[[GlobalEnv]].
      ii. Let thisValue be globalEnv.[[GlobalThisValue]].
    b. Else,
      i. Let thisValue be ! ToObject(thisArgument).
      ii. NOTE: ToObject produces wrapper objects using calleeRealm.
  7. Assert: localEnv is a Function Environment Record.
  8. Perform ! localEnv.BindThisValue(thisValue).
flags: [noStrict, async]
includes: [asyncHelpers.js]
features: [async-iteration]
---*/

var AsyncGeneratorFunction = Object.getPrototypeOf(async function* () {}).constructor;
var global = globalThis;

async function* sloppy() {
  yield this;
  yield (() => this)();
}

async function* strict() {
  'use strict';
  yield this;
  yield (() => this)();
}

async function* inParameter(value = this) {
  yield value;
}

async function* lateRead() {
  yield 0;
  yield this;
}

var viaConstructor = AsyncGeneratorFunction('yield this; yield (() => this)();');

var holder = {
  async *method() { yield this; },
  async *detachedMethod() { yield this; }
};

async function firstValues(it, count) {
  var values = [];
  for (var i = 0; i < count; i++) {
    values.push((await it.next()).value);
  }
  return values;
}

asyncTest(async function () {
  var undefinedThis = await firstValues(sloppy.call(undefined), 2);
  assert.sameValue(undefinedThis[0], global, 'undefined receiver: body');
  assert.sameValue(undefinedThis[1], global, 'undefined receiver: arrow in body');

  assert.sameValue((await sloppy.call(null).next()).value, global, 'null receiver');
  assert.sameValue((await sloppy().next()).value, global, 'implicit receiver');

  var primitives = [true, 1, 'str', Symbol.iterator, 1n];
  for (var i = 0; i < primitives.length; i++) {
    var primitive = primitives[i];
    var wrapped = await firstValues(sloppy.call(primitive), 2);
    assert.sameValue(typeof wrapped[0], 'object', 'primitive receiver ' + String(primitive) + ': wrapped');
    assert.sameValue(wrapped[0].valueOf(), primitive, 'primitive receiver ' + String(primitive) + ': same primitive');
    assert.sameValue(wrapped[1], wrapped[0], 'primitive receiver ' + String(primitive) + ': arrow sees the same wrapper');
  }

  var object = {};
  assert.sameValue((await sloppy.call(object).next()).value, object, 'object receiver is unchanged');

  assert.sameValue((await strict.call(undefined).next()).value, undefined, 'strict: undefined');
  assert.sameValue((await strict.call(null).next()).value, null, 'strict: null');
  assert.sameValue((await strict.call(5).next()).value, 5, 'strict: primitive');
  var strictValues = await firstValues(strict.call(undefined), 2);
  assert.sameValue(strictValues[1], undefined, 'strict: arrow in body');

  assert.sameValue((await inParameter.call(undefined).next()).value, global, 'parameter initializer');

  var late = lateRead.call(undefined);
  await late.next();
  assert.sameValue((await late.next()).value, global, 'read after the first resumption');

  var constructed = await firstValues(viaConstructor.call(undefined), 2);
  assert.sameValue(constructed[0], global, 'AsyncGeneratorFunction-created: body');
  assert.sameValue(constructed[1], global, 'AsyncGeneratorFunction-created: arrow in body');

  assert.sameValue((await holder.method().next()).value, holder, 'method keeps its receiver');
  var detached = holder.detachedMethod;
  assert.sameValue((await detached().next()).value, global, 'detached method');
});
