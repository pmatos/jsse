// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-generator-function-definitions-runtime-semantics-evaluation
description: >
  `yield*` inside an expression the state-machine transform does not lower
  (here a destructuring-assignment default) in an async generator iterates an
  async iterable: the inner values are yielded, sent values are forwarded to the
  inner `next`, and the inner return value is the value of the `yield*`.
info: |
  YieldExpression : yield * AssignmentExpression

  3. Let generatorKind be GetGeneratorKind().
  5. Let iteratorRecord be ? GetIterator(value, generatorKind).
  7. Repeat,
    a. If received.[[Type]] is normal, then
      i. Let innerResult be ? Call(iteratorRecord.[[NextMethod]], iteratorRecord.[[Iterator]], « received.[[Value]] »).
      ii. If generatorKind is async, set innerResult to ? Await(innerResult).
      iii. If innerResult is not an Object, throw a TypeError exception.
      iv. Let done be ? IteratorComplete(innerResult).
      v. If done is true, then
        1. Return ? IteratorValue(innerResult).
      vi. If generatorKind is async, set received to Completion(AsyncGeneratorYield(? IteratorValue(innerResult))).
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration, destructuring-assignment]
---*/

var received = [];
async function* inner() {
  received.push(yield 10);
  received.push(yield 20);
  return 'inner-return';
}
async function* g() {
  var a;
  ({a = yield* inner()} = {});
  return a;
}

asyncTest(async function () {
  var it = g();
  var r1 = await it.next('ignored');
  assert.sameValue(r1.value, 10, 'first inner value');
  assert.sameValue(r1.done, false, 'first step is not done');
  var r2 = await it.next('sent-1');
  assert.sameValue(r2.value, 20, 'second inner value');
  assert.sameValue(r2.done, false, 'second step is not done');
  var r3 = await it.next('sent-2');
  assert.sameValue(r3.value, 'inner-return', 'the yield* evaluates to the inner return value');
  assert.sameValue(r3.done, true, 'the generator completes');
  assert.compareArray(received, ['sent-1', 'sent-2'], 'sent values reach the inner generator');
});
