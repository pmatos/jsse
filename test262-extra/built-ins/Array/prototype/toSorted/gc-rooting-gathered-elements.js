// Copyright (C) 2026 the jsse authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-array.prototype.tosorted
description: >
  Elements read from the receiver stay reachable while later getters and
  the comparator run arbitrary code (including a collection) before the
  result array is created.
info: |
  Array.prototype.toSorted ( comparefn )

  4. Let A be ? ArrayCreate(len).
  ...
  6. Let sortedList be ? SortIndexedProperties(O, len, SortCompare,
     read-through-holes).

  The elements of sortedList are only held by the list while later getters
  and the comparator run.
features: [change-array-by-copy, host-gc-required]
---*/

function churn() {
  $262.gc();
  var junk = [];
  for (var k = 0; k < 1000; k++) junk.push({ x: k });
}

var o = { length: 3 };
[0, 1, 2].forEach(function (i) {
  Object.defineProperty(o, String(i), {
    get: function () { churn(); return { v: 3 - i }; }
  });
});

var result = Array.prototype.toSorted.call(o, function (a, b) { churn(); return a.v - b.v; });
assert.sameValue(result[0].v, 1);
assert.sameValue(result[1].v, 2);
assert.sameValue(result[2].v, 3);
