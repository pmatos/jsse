/*---
description: >
  A yield expression in the test of a switch case clause is a suspension point
  of the generator.
esid: sec-runtime-semantics-caseblockevaluation
info: |
  Runtime Semantics: CaseBlockEvaluation

  CaseClauseIsSelected ( C, input )

  2. Let exprRef be ? Evaluation of the Expression of C.
  3. Let clauseSelector be ? GetValue(exprRef).
  4. Return IsStrictlyEqual(input, clauseSelector).

  A yield expression is a valid AssignmentExpression in a case selector, so
  evaluating it suspends the generator in the middle of CaseBlockEvaluation and
  resumes with the value sent to next().
includes: [compareArray.js]
flags: [async]
features: [generators]
---*/

function* single() {
  switch (1) {
    case (yield 5, 1):
      return 'x';
  }
}

var it = single();
var first = it.next();
assert.sameValue(first.value, 5, 'the selector yield produces its operand');
assert.sameValue(first.done, false, 'the generator is suspended at the selector');
var second = it.next();
assert.sameValue(second.value, 'x', 'the selected clause runs after the selector resumes');
assert.sameValue(second.done, true, 'the generator completes');

assert.compareArray([...single()], [5], 'spreading yields only the selector value');

$DONE();
