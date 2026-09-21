// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-generator-function-definitions-runtime-semantics-evaluation
description: >
  The result of a `yield*` that finishes at a later delegated step is bound to
  its declaration, assignment target or pattern, and an abrupt IteratorValue at
  a later step is catchable by the generator's own try/catch.
info: |
  YieldExpression : yield * AssignmentExpression

  7.a.iv. Let done be ? IteratorComplete(innerResult).
  7.a.v. If done is true, then
    1. Return ? IteratorValue(innerResult).

  The `?` propagates a throw completion into the generator's evaluation, where
  an enclosing try statement can catch it.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration, destructuring-binding]
---*/

function mk(results) {
  var i = 0;
  return {
    [Symbol.asyncIterator]() { return this; },
    next() { return Promise.resolve(results[i++]); }
  };
}

async function collect(it) {
  var out = [];
  while (true) {
    var r = await it.next();
    if (r.done) { break; }
    out.push(r.value);
  }
  return out;
}

asyncTest(async function () {
  async function* declared() {
    const r = yield* mk([{ value: 'a', done: false }, { value: 'V', done: true }]);
    yield 'r=' + r;
  }
  assert.compareArray(await collect(declared()), ['a', 'r=V'], 'const binding, done at step 2');

  async function* assigned() {
    var r;
    r = yield* mk([{ value: 'a', done: false }, { value: 'V', done: true }]);
    yield 'r=' + r;
  }
  assert.compareArray(await collect(assigned()), ['a', 'r=V'], 'assignment target, done at step 2');

  async function* lexicalAssigned() {
    let r;
    r = yield* mk([{ value: 'a', done: false }, { value: 'V', done: true }]);
    yield 'r=' + r;
  }
  assert.compareArray(await collect(lexicalAssigned()), ['a', 'r=V'], 'let assignment target, done at step 2');

  async function* destructured() {
    var { x } = yield* mk([{ value: 'a', done: false }, { value: { x: 'X' }, done: true }]);
    yield 'x=' + x;
  }
  assert.compareArray(await collect(destructured()), ['a', 'x=X'], 'pattern binding, done at step 2');

  async function* firstStep() {
    const r = yield* mk([{ value: 'V1', done: true }]);
    yield 'r=' + r;
  }
  assert.compareArray(await collect(firstStep()), ['r=V1'], 'const binding, done at step 1');

  var err = new Error('value getter');
  async function* caught() {
    try {
      yield* mk([{ value: 'a', done: false }, { done: false, get value() { throw err; } }]);
    } catch (e) {
      yield 'caught:' + (e === err);
    }
    yield 'end';
  }
  assert.compareArray(
    await collect(caught()),
    ['a', 'caught:true', 'end'],
    'a throwing IteratorValue at step 2 is catchable by the generator'
  );
});
