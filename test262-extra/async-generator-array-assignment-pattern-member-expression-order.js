// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-iteratordestructuringassignmentevaluation
description: >
  A member-expression target inside an array-assignment pattern has its
  reference (base, then computed key) evaluated before the pattern accesses
  the iterator for that element and before any Initializer -- including when
  the computed key expression itself contains the `await` that forces the
  pattern through state-machine lowering.
info: |
  AssignmentElement : DestructuringAssignmentTarget Initializer?

  1. If DestructuringAssignmentTarget is neither an ObjectLiteral nor an
     ArrayLiteral, then
    a. Let lRef be ? Evaluation of DestructuringAssignmentTarget.
  2. Let value be undefined.
  3. If iteratorRecord.[[Done]] is false, then
    a. Let next be ? IteratorStepValue(iteratorRecord).
    [...]
  4. If Initializer is present and value is undefined, then
    [...]

  NOTE: Left to right evaluation order is maintained by evaluating a
  DestructuringAssignmentTarget that is not a destructuring pattern prior to
  accessing the iterator or evaluating the Initializer.
flags: [async]
includes: [asyncHelpers.js]
features: [async-iteration, destructuring-assignment]
---*/

asyncTest(async function () {
  // A member-expression target with a default: the reference's base (the
  // call that produces the object) is evaluated first, before the default
  // await, which is itself evaluated before the iterator access completes
  // the element.
  var log = [];
  var obj = {};
  Object.defineProperty(obj, 'prop', {
    set: function (v) {
      log.push('set:' + v);
    },
  });
  function getTarget() {
    log.push('getTarget');
    return obj;
  }
  [getTarget().prop = await (log.push('default-eval'), 99)] = [];
  assert.sameValue(
    log.join(','),
    'getTarget,default-eval,set:99',
    'the base reference is captured before the default await runs, and the write happens last'
  );

  // A computed key containing its own `await`: the key expression is part
  // of evaluating the reference itself, so it runs before the iterator is
  // even accessed for this element.
  var log2 = [];
  var keyHolder = {
    get k() {
      log2.push('get-k');
      return 'key';
    },
  };
  var target = {};
  var arr = [123];
  [target[await (log2.push('await-k'), keyHolder.k)]] = arr;
  assert.sameValue(
    log2.join(','),
    'await-k,get-k',
    'the computed key (including its own await) is evaluated before the array is stepped'
  );
  assert.sameValue(target.key, 123, 'the element value is written to the captured reference');
});
