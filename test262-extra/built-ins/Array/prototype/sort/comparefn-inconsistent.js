// Copyright (C) 2026 the jsse authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-sortindexedproperties
description: >
  A comparefn that is not a consistent comparator only makes the sort order
  implementation-defined; sort still completes with a permutation of the
  original elements.
info: |
  SortIndexedProperties ( obj, len, SortCompare, holes )

  4. Sort items using an implementation-defined sequence of calls to
     SortCompare. If any such call returns an abrupt completion, stop before
     performing any further calls to SortCompare and return that Completion
     Record.
  5. Return items.

  The sort order is implementation-defined if comparefn is not a consistent
  comparator for the elements of items (see 23.1.3.30).
---*/

var n = 1000;
var a = [];
for (var i = 0; i < n; i++) a.push(i * 7 % (n + 1));

// Alternating results contradict themselves for any fixed pair of elements.
var flip = 0;
a.sort(function () {
  return (flip++ % 2) ? 1 : -1;
});

assert.sameValue(a.length, n);
var seen = [];
for (var i = 0; i < n; i++) seen[a[i]] = true;
for (var i = 0; i < n; i++) {
  assert.sameValue(seen[i * 7 % (n + 1)], true, "element " + i + " retained");
}

var calls = 0;
var b = [];
for (var i = 0; i < n; i++) b.push(n - i);
assert.throws(Test262Error, function () {
  b.sort(function (x, y) {
    if (++calls === 500) throw new Test262Error();
    return calls % 3 - 1;
  });
});
assert.sameValue(calls, 500, "no SortCompare calls after the abrupt completion");
