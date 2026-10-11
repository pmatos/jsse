// Copyright (C) 2026 the jsse authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-%typedarray%.prototype.sort
description: >
  A comparefn that is not a consistent comparator only makes the order of
  %TypedArray%.prototype.sort implementation-defined; it still completes with
  a permutation of the original elements, and stops at the first abrupt
  completion.
info: |
  SortIndexedProperties ( obj, len, SortCompare, holes )

  4. Sort items using an implementation-defined sequence of calls to
     SortCompare. If any such call returns an abrupt completion, stop before
     performing any further calls to SortCompare and return that Completion
     Record.

  The sort order is implementation-defined if comparefn is not a consistent
  comparator for the elements of items (see 23.1.3.30).
includes: [testTypedArray.js]
features: [BigInt, TypedArray]
---*/

var n = 1000;

testWithBigIntTypedArrayConstructors(function (TA) {
  var ta = new TA(n);
  for (var i = 0; i < n; i++) ta[i] = BigInt(i % 100);

  // Alternating results contradict themselves for any fixed pair of elements.
  var flip = 0;
  var result = ta.sort(function () {
    return (flip++ % 2) ? 1 : -1;
  });

  assert.sameValue(result.length, n);
  var counts = [];
  for (var i = 0; i < n; i++) counts[Number(result[i])] = (counts[Number(result[i])] || 0) + 1;
  for (var v = 0; v < 100; v++) {
    assert.sameValue(counts[v], n / 100, "value " + v + " retained");
  }

  var calls = 0;
  assert.throws(Test262Error, function () {
    ta.sort(function () {
      if (++calls === 500) throw new Test262Error();
      return calls % 3 - 1;
    });
  });
  assert.sameValue(calls, 500, "no SortCompare calls after the abrupt completion");
});
