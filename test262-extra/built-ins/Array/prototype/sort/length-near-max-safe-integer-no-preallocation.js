// Copyright (C) 2026 the jsse authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-array.prototype.sort
description: >
  A huge array-like length is not eagerly materialised; the abrupt
  completion from reading the first element propagates.
info: |
  Array.prototype.sort ( comparefn )

  3. Let len be ? LengthOfArrayLike(obj).
  ...
  5. Let sortedList be ? SortIndexedProperties(obj, len, SortCompare,
     skip-holes).
---*/

var obj = {
  length: Math.pow(2, 53) - 1,
  get 0() { throw new Test262Error(); }
};
assert.throws(Test262Error, function () {
  Array.prototype.sort.call(obj);
});
