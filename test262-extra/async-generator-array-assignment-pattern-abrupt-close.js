// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-destructuringassignmentevaluation
description: >
  A rejected `await` in an array-assignment pattern element's default closes
  the pattern's own iterator exactly once (IteratorClose on abrupt
  completion), distinct from a synchronous throw mid-walk.
info: |
  ArrayAssignmentPattern : [ AssignmentElementList ]

  1. Let iteratorRecord be ? GetIterator(value, sync).
  2. Let result be Completion(IteratorDestructuringAssignmentEvaluation of
     AssignmentElementList with argument iteratorRecord).
  3. If iteratorRecord.[[Done]] is false, return ? IteratorClose(iteratorRecord, result).
  4. Return ? result.
flags: [async]
includes: [asyncHelpers.js]
features: [async-iteration, destructuring-assignment]
---*/

function makeCountingIterable(values) {
  var returnCalls = 0;
  return {
    returnCalls: function () { return returnCalls; },
    [Symbol.iterator]: function () {
      var i = 0;
      return {
        next: function () {
          return i < values.length ? { value: values[i++], done: false } : { value: undefined, done: true };
        },
        return: function (v) {
          returnCalls++;
          return { done: true, value: v };
        },
      };
    },
  };
}

asyncTest(async function () {
  var it = makeCountingIterable([undefined, 2, 3]);
  var a;
  var error;
  try {
    [a = await Promise.reject(new Error('nope'))] = it;
  } catch (e) {
    error = e;
  }
  assert.notSameValue(error, undefined, 'the rejected default propagates as a thrown error');
  assert.sameValue(error.message, 'nope');
  assert.sameValue(
    it.returnCalls(),
    1,
    'the iterator is closed exactly once after the rejected default'
  );

  // A rejection on a later element closes the iterator just as well, after
  // the earlier elements already stepped it without closing.
  var it2 = makeCountingIterable([1, undefined, 3]);
  var x, y;
  var error2;
  try {
    [x, y = await Promise.reject(new Error('later')), ] = it2;
  } catch (e) {
    error2 = e;
  }
  assert.notSameValue(error2, undefined);
  assert.sameValue(x, 1, 'the first element bound before the rejection');
  assert.sameValue(
    it2.returnCalls(),
    1,
    'the iterator is closed exactly once even when an earlier element already stepped it'
  );
});
