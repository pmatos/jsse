// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-assignment-operators-runtime-semantics-evaluation
description: >
  The completion value of an array-assignment expression whose pattern
  contains an `await` default is the right-hand-side value, not the
  destructured result (AssignmentExpression : LeftHandSideExpression `=`
  AssignmentExpression, step "Return rval"), matching the already-correct
  object-assignment-pattern case.
info: |
  AssignmentExpression : LeftHandSideExpression = AssignmentExpression

  [...]
  6. Let rval be ? GetValue(rref).
  7. If LeftHandSideExpression is either an ObjectLiteral or an ArrayLiteral, then
    a. Let nestedAssignmentPattern be the AssignmentPattern that is covered by
       LeftHandSideExpression.
    b. Perform ? DestructuringAssignmentEvaluation of nestedAssignmentPattern
       with argument rval.
    c. Return rval.
flags: [async]
includes: [asyncHelpers.js]
features: [async-iteration, destructuring-assignment]
---*/

asyncTest(async function () {
  var a;
  var arr = [1];
  var r = ([a = await 5] = arr);
  assert.sameValue(r, arr, 'the completion value is the RHS array, not the destructured element');
  assert.sameValue(a, 1, 'the element itself is still bound normally');

  var b;
  var arr2 = [undefined];
  var r2 = ([b = await 9] = arr2);
  assert.sameValue(r2, arr2, 'the completion value is the RHS array even when the default fires');
  assert.sameValue(b, 9, 'the default value is bound when the stepped value is undefined');

  async function* g() {
    var c;
    var src = [2];
    var result = ([c = await 3] = src);
    yield result === src;
    yield c;
  }
  var it = g();
  assert.sameValue((await it.next()).value, true, 'inside an async generator, the completion value is still the RHS');
  assert.sameValue((await it.next()).value, 2, 'and the element binding is unaffected');
});
