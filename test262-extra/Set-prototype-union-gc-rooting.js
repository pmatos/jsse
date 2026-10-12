// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-set.prototype.union
description: >
  The values Set.prototype.union has put in resultSetData must stay reachable
  while the keys iterator runs: a value copied from O that the iterator then
  deletes from O, and a fresh value the iterator produced, are held only by
  resultSetData when a later next() call runs a garbage collection.
info: |
  Set.prototype.union ( other )
  4. Let keysIter be ? GetIteratorFromMethod(otherRec.[[SetObject]], otherRec.[[Keys]]).
  5. Let resultSetData be a copy of O.[[SetData]].
  6. Let next be not-started.
  7. Repeat, while next is not done,
    a. Set next to ? IteratorStepValue(keysIter).
    b. If next is not done, then
      i. Set next to CanonicalizeKeyedCollectionKey(next).
      ii. If SetDataHas(resultSetData, next) is false, then
        1. Append next to resultSetData.
features: [set-methods, host-gc-required]
---*/

function collect() {
  for (var i = 0; i < 1000; i++) {
    [{}, {}, {}, {}];
  }
  $262.gc();
}

var copied = { tag: "copied" };
var receiver = new Set([copied]);
copied = null;
var steps = 0;
var result = receiver.union({
  size: 1,
  has: function () {
    throw new Test262Error("has should not be called");
  },
  keys: function () {
    return {
      next: function () {
        steps++;
        if (steps === 1) {
          receiver.clear();
          return { done: false, value: { tag: "fresh" } };
        }
        collect();
        return { done: true };
      }
    };
  }
});

var values = Array.from(result);
assert.sameValue(values.length, 2, "result size");
assert.sameValue(values[0].tag, "copied", "the value copied from O survived the collection");
assert.sameValue(values[1].tag, "fresh", "the value the iterator produced survived the collection");
