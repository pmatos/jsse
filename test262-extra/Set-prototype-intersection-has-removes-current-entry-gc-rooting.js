// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-set.prototype.intersection
description: >
  The value Set.prototype.intersection is passing to otherRec.[[Has]] must stay
  reachable while that call runs, even when the call removes it from O and runs
  a garbage collection: a strict function with no parameters does not hold
  its argument, and the value is appended to resultSetData once the call returns.
info: |
  Set.prototype.intersection ( other )
  6. If SetDataSize(O.[[SetData]]) ≤ otherRec.[[Size]], then
    c. Repeat, while index < thisSize,
      i. Let e be O.[[SetData]][index].
      iii. If e is not empty, then
        1. Let inOther be ToBoolean(? Call(otherRec.[[Has]], otherRec.[[SetObject]], « e »)).
        2. If inOther is true, then
          c. If SetDataHas(resultSetData, e) is false, then
            i. Append e to resultSetData.
features: [set-methods, host-gc-required]
---*/

var kept = { tag: "kept" };
var receiver = new Set([kept]);
kept = null;
var result = receiver.intersection({
  size: 10,
  has: function () {
    "use strict";
    receiver.clear();
    for (var i = 0; i < 1000; i++) {
      [{}, {}, {}, {}];
    }
    $262.gc();
    return true;
  },
  keys: function () {
    throw new Test262Error("keys should not be called");
  }
});

var values = Array.from(result);
assert.sameValue(values.length, 1, "result size");
assert.sameValue(typeof values[0], "object", "the value is still an object");
assert.sameValue(values[0].tag, "kept", "the value passed to has survived the collection");
