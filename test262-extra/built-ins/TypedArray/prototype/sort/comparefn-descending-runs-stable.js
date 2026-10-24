// Copyright (C) 2026 the jsse authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-%typedarray%.prototype.sort
description: >
  %TypedArray%.prototype.sort with a comparefn is correct and stable for input
  that is already sorted, sorted in reverse, or made of mixed ascending and
  descending runs, and a comparefn that throws part-way through such input
  leaves a permutation of the original elements.
info: |
  SortIndexedProperties ( obj, len, SortCompare, holes )

  4. Sort items using an implementation-defined sequence of calls to
     SortCompare. If any such call returns an abrupt completion, stop before
     performing any further calls to SortCompare and return that Completion
     Record.

  23.2.3.29 %TypedArray%.prototype.sort ( comparefn )

  The sort must be stable.
includes: [testTypedArray.js, compareArray.js]
features: [TypedArray]
---*/

function check(TA, input, expected, label) {
  var ta = new TA(input);
  assert(compareArray(ta.sort(function (a, b) { return a - b; }), expected), label + " ascending");
  var desc = new TA(input);
  assert(
    compareArray(desc.sort(function (a, b) { return b - a; }), expected.slice().reverse()),
    label + " descending"
  );
}

testWithTypedArrayConstructors(function (TA) {
  check(TA, [1, 2, 3, 4, 5, 6, 7], [1, 2, 3, 4, 5, 6, 7], "sorted");
  check(TA, [7, 6, 5, 4, 3, 2, 1], [1, 2, 3, 4, 5, 6, 7], "reverse-sorted");
  check(TA, [1, 3, 5, 7, 6, 4, 2], [1, 2, 3, 4, 5, 6, 7], "ascending then descending");
  check(TA, [5, 6, 7, 4, 3, 2, 1, 8, 9], [1, 2, 3, 4, 5, 6, 7, 8, 9], "mixed runs");
  check(TA, [2, 1], [1, 2], "two descending");
  check(TA, [3, 3, 3, 2, 2, 1], [1, 2, 2, 3, 3, 3], "descending with duplicates");
});

testWithTypedArrayConstructors(function (TA) {
  // Equal keys (same value >> 1) must keep their original relative order, also
  // inside a strictly descending prefix of the key order.
  var ta = new TA([9, 8, 7, 6, 5, 4, 3, 2]);
  ta.sort(function (a, b) {
    return (b >> 1) - (a >> 1);
  });
  assert(compareArray(ta, [9, 8, 7, 6, 5, 4, 3, 2]), "descending by key keeps stable order");

  var asc = new TA([9, 8, 7, 6, 5, 4, 3, 2]);
  asc.sort(function (a, b) {
    return (a >> 1) - (b >> 1);
  });
  assert(compareArray(asc, [3, 2, 5, 4, 7, 6, 9, 8]), "ascending by key keeps stable order");
});

testWithTypedArrayConstructors(function (TA) {
  var ta = new TA([8, 7, 6, 5, 4, 3, 2, 1]);
  var calls = 0;
  assert.throws(Test262Error, function () {
    ta.sort(function (a, b) {
      if (++calls === 5) throw new Test262Error();
      return b - a;
    });
  });
  assert.sameValue(calls, 5, "no SortCompare calls after the abrupt completion");
  var seen = Array.prototype.slice.call(ta).sort(function (a, b) { return a - b; });
  assert(compareArray(seen, [1, 2, 3, 4, 5, 6, 7, 8]), "elements are unchanged as a multiset");
});
