/*---
description: >
  A call expression as the left-hand side of a for-in/for-of head is accepted
  by sloppy code for web compatibility. The call runs and a ReferenceError is
  thrown, also when the loop is in a generator or async function and so runs
  on the suspending state machine (it used to be skipped without an error).
esid: sec-for-in-and-for-of-statements
info: |
  The early error of 13.15.1 for a call target is not applied to sloppy code
  by the engines; the loop head is evaluated as a reference and PutValue on a
  non-reference throws a ReferenceError (6.2.5.6).
flags: [noStrict, async]
includes: [asyncHelpers.js]
features: [async-functions, generators, async-iteration]
---*/

var calls = 0;
function f() { calls++; return {}; }

function* gen(kind) {
  if (kind === "of") {
    for (f() of [1]) yield 1;
  } else {
    for (f() in { k: 1 }) yield 1;
  }
}

assert.throws(ReferenceError, function () { gen("of").next(); }, "generator for-of");
assert.sameValue(calls, 1, "the call of a generator for-of ran");
assert.throws(ReferenceError, function () { gen("in").next(); }, "generator for-in");
assert.sameValue(calls, 2, "the call of a generator for-in ran");

asyncTest(async function () {
  await assert.throwsAsync(ReferenceError, async function () {
    for (f() of [1]) await 0;
  }, "async for-of");
  assert.sameValue(calls, 3, "the call of an async for-of ran");

  await assert.throwsAsync(ReferenceError, async function () {
    for (f() in { k: 1 }) await 0;
  }, "async for-in");
  assert.sameValue(calls, 4, "the call of an async for-in ran");

  await assert.throwsAsync(ReferenceError, async function () {
    for await (f() of [1]) ;
  }, "for await");
  assert.sameValue(calls, 5, "the call of a for await ran");

  var closed = 0;
  var iterable = {
    [Symbol.iterator]() {
      return { next() { return { value: 1, done: false }; }, return() { closed++; return {}; } };
    },
  };
  await assert.throwsAsync(ReferenceError, async function () {
    for (f() of iterable) await 0;
  }, "async for-of over an iterator with return");
  assert.sameValue(closed, 1, "the iterator was closed");

  var empty = 0;
  async function none() {
    for (f() of []) await 0;
    empty++;
  }
  await none();
  assert.sameValue(empty, 1, "an empty iterable never evaluates the head");
});
