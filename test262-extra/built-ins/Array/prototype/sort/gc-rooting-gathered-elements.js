// Copyright (C) 2026 the jsse authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-array.prototype.sort
description: >
  Elements read from the receiver stay reachable while later getters,
  the comparator and setters run arbitrary code (including a collection)
  before they are written back.
info: |
  Array.prototype.sort ( comparefn )

  5. Let sortedList be ? SortIndexedProperties(obj, len, SortCompare,
     skip-holes).
  ...
  8. Repeat, while j < itemCount,
    a. Perform ? Set(obj, ! ToString(𝔽(j)), sortedList[j], true).

  The elements of sortedList are only held by the list while the getters
  (step 5) and the setters (step 8.a) run.
features: [host-gc-required]
---*/

function churn() {
  $262.gc();
  var junk = [];
  for (var k = 0; k < 1000; k++) junk.push({ x: k });
}

var store = {};
var o = { length: 3 };
[0, 1, 2].forEach(function (i) {
  Object.defineProperty(o, String(i), {
    get: function () { churn(); return { v: 3 - i }; },
    set: function (v) { store[i] = v; churn(); },
    configurable: true
  });
});

Array.prototype.sort.call(o, function (a, b) { churn(); return a.v - b.v; });
assert.sameValue(store[0].v, 1);
assert.sameValue(store[1].v, 2);
assert.sameValue(store[2].v, 3);
