// Copyright (C) 2026 the jsse authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-array.prototype.sort
description: >
  Array.prototype.sort and Array.prototype.toSorted with a comparefn are
  correct and stable for sorted, reverse-sorted and mixed ascending and
  descending input, and undefined elements and holes still sort last.
includes: [compareArray.js]
---*/

function asc(a, b) { return a - b; }
function desc(a, b) { return b - a; }

assert(compareArray([1, 2, 3, 4, 5].sort(asc), [1, 2, 3, 4, 5]), "sorted");
assert(compareArray([5, 4, 3, 2, 1].sort(asc), [1, 2, 3, 4, 5]), "reverse-sorted");
assert(compareArray([1, 3, 5, 4, 2].sort(asc), [1, 2, 3, 4, 5]), "ascending then descending");
assert(compareArray([5, 6, 7, 4, 3, 2, 1, 8, 9].toSorted(desc), [9, 8, 7, 6, 5, 4, 3, 2, 1]), "toSorted mixed runs");

var keyed = [{k: 3, id: 0}, {k: 3, id: 1}, {k: 2, id: 2}, {k: 2, id: 3}, {k: 1, id: 4}, {k: 1, id: 5}];
var ids = keyed.slice().sort(function (a, b) { return a.k - b.k; }).map(function (o) { return o.id; });
assert(compareArray(ids, [4, 5, 2, 3, 0, 1]), "equal keys keep input order inside descending runs");

var withGaps = [3, undefined, 1, , 2];
withGaps.sort(desc);
assert.sameValue(withGaps[0], 3);
assert.sameValue(withGaps[1], 2);
assert.sameValue(withGaps[2], 1);
assert.sameValue(withGaps[3], undefined, "undefined sorts after defined values");
assert.sameValue(4 in withGaps, false, "hole sorts last");
