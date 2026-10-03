// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-iterator.prototype.toarray
description: >
  Each value Iterator.prototype.toArray appends to its result list (step
  5.c, "Append value to items") must stay reachable across every later
  iteration of the Repeat loop -- a later next() call can run arbitrary
  script, and therefore trigger a garbage collection, before
  CreateArrayFromList ever runs.
info: |
  %Iterator.prototype%.toArray ( )
  5. Repeat,
    a. Let value be ? IteratorStepValue(iterated).
    b. If value is done, return CreateArrayFromList(items).
    c. Append value to items.
features: [host-gc-required]
---*/
var i = 0;
class ChurnIterator extends Iterator {
  next() {
    $262.gc();
    for (var j = 0; j < 200; j++) {
      [{}, {}, {}, {}];
    }
    if (i >= 5) {
      return { done: true, value: undefined };
    }
    var v = { tag: i };
    i++;
    return { done: false, value: v };
  }
}

var arr = new ChurnIterator().toArray();
assert.sameValue(arr.length, 5);
for (var k = 0; k < 5; k++) {
  assert.sameValue(arr[k].tag, k, "element " + k + " lost its identity");
}
