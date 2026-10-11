// Copyright (C) 2026 the jsse authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-array.prototype.tosorted
description: >
  A length of 2^32-1 passes the ArrayCreate check without eagerly
  materialising every element; the abrupt completion from reading the first
  element propagates.
info: |
  Array.prototype.toSorted ( comparefn )

  3. Let len be ? LengthOfArrayLike(O).
  4. Let A be ? ArrayCreate(len).
  ...
  6. Let sortedList be ? SortIndexedProperties(O, len, SortCompare,
     read-through-holes).
features: [change-array-by-copy]
---*/

var obj = {
  length: Math.pow(2, 32) - 1,
  get 0() { throw new Test262Error(); }
};
assert.throws(Test262Error, function () {
  Array.prototype.toSorted.call(obj);
});
