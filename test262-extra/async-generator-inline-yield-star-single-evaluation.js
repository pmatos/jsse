// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-generator-function-definitions-runtime-semantics-evaluation
description: >
  A `yield*` inside an expression the state-machine transform does not lower
  evaluates its operand once and starts the delegate once, however many steps
  the delegate takes.
info: |
  YieldExpression : yield * AssignmentExpression

  1. Let generatorKind be GetGeneratorKind().
  2. Let exprRef be ? Evaluation of AssignmentExpression.
  3. Let value be ? GetValue(exprRef).
  5. Let iteratorRecord be ? GetIterator(value, generatorKind).
flags: [async]
includes: [asyncHelpers.js]
features: [async-iteration, destructuring-assignment]
---*/

var operandEvaluations = 0;
var delegateStarts = 0;
async function* inner() {
  delegateStarts++;
  yield 1;
  yield 2;
  yield 3;
  return 'done';
}
function operand() {
  operandEvaluations++;
  return inner();
}
async function* g() {
  var a;
  ({a = yield* operand()} = {});
  return a;
}

asyncTest(async function () {
  var it = g();
  var values = [];
  var r;
  do {
    r = await it.next();
    values.push(r.value);
  } while (!r.done);
  assert.sameValue(values.join(), '1,2,3,done', 'every delegated step is observed');
  assert.sameValue(operandEvaluations, 1, 'the operand is evaluated once');
  assert.sameValue(delegateStarts, 1, 'the delegate starts once');
});
