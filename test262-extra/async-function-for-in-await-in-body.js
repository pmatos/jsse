/*---
description: >
  An async function for-in loop whose body (or RHS) awaits still enumerates the
  object, with per-iteration bindings and correct loop control.
esid: sec-runtime-semantics-forinofloopevaluation
info: |
  ForIn/OfBodyEvaluation steps the enumerator, binds each key and evaluates the
  body. Await suspends the async function without losing the enumerator or the
  active iteration environment.
flags: [async]
includes: [compareArray.js, asyncHelpers.js]
features: [async-functions]
---*/

async function plain(o) {
  var seen = [];
  for (var k in o) {
    await 0;
    seen.push(k);
  }
  seen.push('after');
  return seen;
}

async function closures(o) {
  var fns = [];
  for (let k in o) {
    await 0;
    fns.push(function () { return k; });
  }
  return fns.map(function (f) { return f(); });
}

async function nested(o, p) {
  var seen = [];
  for (const a in o) {
    for (const b in p) {
      await 0;
      seen.push(a + b);
    }
  }
  return seen;
}

async function control(o) {
  var seen = [];
  outer: for (var i of [1, 2]) {
    for (var k in o) {
      await 0;
      if (k === 'b') continue outer;
      if (i === 2 && k === 'a') break outer;
      seen.push(i + k);
    }
  }
  return seen;
}

async function nullish(o) {
  var ran = false;
  for (var k in o) {
    await 0;
    ran = true;
  }
  return ran;
}

async function rhsAwaits() {
  var seen = [];
  for (var k in await Promise.resolve({ r: 1, s: 2 })) seen.push(k);
  return seen;
}

async function rejectsInBody(o) {
  for (var k in o) {
    await Promise.reject(new Test262Error('rejected'));
  }
}

async function caught(o) {
  try {
    for (var k in o) {
      await 0;
      throw new Test262Error('in body');
    }
  } catch (e) {
    return e.message;
  }
}

async function returns(o) {
  for (var k in o) {
    await 0;
    return k;
  }
}

asyncTest(async function () {
  assert.compareArray(await plain({ a: 1, b: 2 }), ['a', 'b', 'after'], 'await in body');
  assert.compareArray(await closures({ ab: 1, cd: 2 }), ['ab', 'cd'], 'per-iteration bindings');
  assert.compareArray(await nested({ x: 1, y: 1 }, { 1: 1, 2: 1 }), ['x1', 'x2', 'y1', 'y2'], 'nested');
  assert.compareArray(await control({ a: 1, b: 2 }), ['1a'], 'labelled continue and break');
  assert.sameValue(await nullish(undefined), false, 'undefined RHS skips the body');
  assert.compareArray(await rhsAwaits(), ['r', 's'], 'await in the RHS');
  await assert.throwsAsync(Test262Error, function () { return rejectsInBody({ a: 1 }); }, 'rejection');
  assert.sameValue(await caught({ a: 1 }), 'in body', 'throw caught outside the loop');
  assert.sameValue(await returns({ a: 1, b: 2 }), 'a', 'return from inside the loop');
});
