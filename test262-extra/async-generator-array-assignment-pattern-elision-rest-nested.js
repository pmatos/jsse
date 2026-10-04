// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-iteratordestructuringassignmentevaluation
description: >
  An array-assignment pattern containing an `await` default also lowers its
  elision, rest, and nested-pattern elements correctly: an elision still
  steps the iterator and discards the value, a rest element drains the
  iterator to exhaustion, and a nested array/object pattern element recurses
  into its own destructuring evaluation.
info: |
  Elision : `,`

  1. If iteratorRecord.[[Done]] is false, then
    a. Perform ? IteratorStep(iteratorRecord).
  2. Return unused.

  AssignmentRestElement : `...` DestructuringAssignmentTarget

  1. [...]
  2. Let A be ! ArrayCreate(0).
  3. Let n be 0.
  4. Repeat, while iteratorRecord.[[Done]] is false,
    a. Let next be ? IteratorStepValue(iteratorRecord).
    b. If next is not done, then
      i. Perform ! CreateDataPropertyOrThrow(A, ! ToString(F(n)), next).
      ii. Set n to n + 1.

  AssignmentElement : DestructuringAssignmentTarget Initializer?

  [...]
  6. If DestructuringAssignmentTarget is either an ObjectLiteral or an
     ArrayLiteral, then
    a. Let nestedAssignmentPattern be the AssignmentPattern that is covered
       by DestructuringAssignmentTarget.
    b. Return ? DestructuringAssignmentEvaluation of nestedAssignmentPattern
       with argument v.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration, destructuring-assignment]
---*/

asyncTest(async function () {
  // Elision alongside an await default.
  var a;
  [, a = await 1] = [99, undefined];
  assert.sameValue(a, 1, 'the elided first element is skipped and the default fills the second');

  // Rest element alongside an await default earlier in the pattern.
  var b, rest;
  [b = await 2, ...rest] = [undefined, 3, 4];
  assert.sameValue(b, 2, 'the default fires for the first element');
  assert.compareArray(rest, [3, 4], 'the rest element drains the remaining values');

  // Nested array pattern as the element whose default contains the await.
  var nested;
  [[nested] = await Promise.resolve([5])] = [];
  assert.sameValue(nested, 5, 'the default supplies an array destructured by the nested pattern');

  // Nested object pattern, same shape.
  var nestedObj;
  [{ nestedObj } = await Promise.resolve({ nestedObj: 6 })] = [];
  assert.sameValue(nestedObj, 6, 'the default supplies an object destructured by the nested pattern');

  // Nested pattern inside a rest element whose own default contains the await.
  var c;
  [...[c = await 7]] = [];
  assert.sameValue(c, 7, 'a nested default inside a rest-drained array still suspends correctly');
});
