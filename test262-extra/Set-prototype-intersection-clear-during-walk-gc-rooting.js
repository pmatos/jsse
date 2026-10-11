// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-set.prototype.intersection
description: >
  A value Set.prototype.intersection has already appended to resultSetData
  must stay reachable when a later otherRec.[[Has]] call clears O and runs a
  garbage collection: after the clear, resultSetData is the only thing holding
  it, and the walk goes on to the elements added after the clear.
info: |
  Set.prototype.intersection ( other )
  6. If SetDataSize(O.[[SetData]]) ≤ otherRec.[[Size]], then
    a. Let thisSize be the number of elements in O.[[SetData]].
    b. Let index be 0.
    c. Repeat, while index < thisSize,
      i. Let e be O.[[SetData]][index].
      ii. Set index to index + 1.
      iii. If e is not empty, then
        1. Let inOther be ToBoolean(? Call(otherRec.[[Has]], otherRec.[[SetObject]], « e »)).
        2. If inOther is true, then
          ...
          c. If SetDataHas(resultSetData, e) is false, then
            i. Append e to resultSetData.
features: [set-methods, host-gc-required]
---*/

var kept = { tag: "kept" };
var receiver = new Set([kept, 2]);
kept = null;
var calls = 0;
var result = receiver.intersection({
  size: 10,
  has: function () {
    calls++;
    if (calls === 1) {
      receiver.clear();
      receiver.add(9);
    } else {
      for (var i = 0; i < 1000; i++) {
        [{}, {}, {}, {}];
      }
      $262.gc();
    }
    return true;
  },
  keys: function () {
    throw new Test262Error("keys should not be called");
  }
});

var values = Array.from(result);
assert.sameValue(values.length, 2, "result size");
assert.sameValue(typeof values[0], "object", "the value kept before the clear is still an object");
assert.sameValue(values[0].tag, "kept", "the value kept before the clear survived the collection");
assert.sameValue(values[1], 9, "the value added after the clear is visited");
