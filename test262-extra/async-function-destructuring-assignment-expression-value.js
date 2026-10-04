/*---
description: >
  A destructuring-assignment expression containing an await still evaluates
  to the right-hand-side value (not undefined, and not the awaited default),
  for both the array and object forms.
esid: sec-assignment-operators-runtime-semantics-evaluation
info: |
  AssignmentExpression : LeftHandSideExpression = AssignmentExpression

  ...
  6. Let rval be ? GetValue(rref).
  7. If LeftHandSideExpression is either an ObjectLiteral or an ArrayLiteral, then
    a. Let status be Completion(DestructuringAssignmentEvaluation of
       LeftHandSideExpression with argument rval).
    ...
    c. Return rval.
flags: [async]
features: [async-functions, destructuring-assignment]
---*/

async function viaArray() {
  var a;
  return [a = await 1] = [];
}

async function viaObject() {
  var a;
  return ({ a = await 1 } = {});
}

Promise.all([viaArray(), viaObject()]).then(function (results) {
  assert.compareArray(results[0], [], 'the array form evaluates to the RHS array');
  assert.sameValue(Object.keys(results[1]).length, 0, 'the object form evaluates to the RHS object');
}).then($DONE, $DONE);
