// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-set.prototype.difference
description: >
  resultSetData, the copy of O.[[SetData]] that Set.prototype.difference
  filters, must stay reachable while otherRec.[[Has]] or the keys iterator
  runs: once user code clears O and runs a garbage collection, the copy is the
  only thing holding the values it has not visited yet.
info: |
  Set.prototype.difference ( other )
  4. Let resultSetData be a copy of O.[[SetData]].
  5. If SetDataSize(O.[[SetData]]) ≤ otherRec.[[Size]], then
    ...
    c. Repeat, while index < thisSize,
      i. Let e be resultSetData[index].
      ii. If e is not EMPTY, then
        1. Let inOther be ToBoolean(? Call(otherRec.[[Has]], otherRec.[[SetObject]], « e »)).
        2. If inOther is true, then
          a. Set resultSetData[index] to EMPTY.
      iii. Set index to index + 1.
  6. Else,
    a. Let keysIter be ? GetIteratorFromMethod(otherRec.[[SetObject]], otherRec.[[Keys]]).
    ...
features: [set-methods, host-gc-required]
---*/

function collect() {
  for (var i = 0; i < 1000; i++) {
    [{}, {}, {}, {}];
  }
  $262.gc();
}

var first = { tag: "first" };
var second = { tag: "second" };
var viaHas = new Set([first, second]);
first = null;
second = null;
var hasCalls = 0;
var hasResult = viaHas.difference({
  size: 10,
  has: function () {
    hasCalls++;
    if (hasCalls === 1) {
      viaHas.clear();
      collect();
    }
    return false;
  },
  keys: function () {
    throw new Test262Error("keys should not be called");
  }
});
var hasValues = Array.from(hasResult);
assert.sameValue(hasValues.length, 2, "has branch: result size");
assert.sameValue(hasValues[0].tag, "first", "has branch: the visited value survived");
assert.sameValue(hasValues[1].tag, "second", "has branch: the value not yet visited survived");

var kept = { tag: "kept" };
var viaKeys = new Set([kept, 2, 3]);
kept = null;
var steps = 0;
var keysResult = viaKeys.difference({
  size: 0,
  has: function () {
    throw new Test262Error("has should not be called");
  },
  keys: function () {
    return {
      next: function () {
        steps++;
        if (steps === 1) {
          viaKeys.clear();
          return { done: false, value: 3 };
        }
        collect();
        return { done: true };
      }
    };
  }
});
var keysValues = Array.from(keysResult);
assert.sameValue(keysValues.length, 2, "keys branch: result size");
assert.sameValue(keysValues[0].tag, "kept", "keys branch: the copied value survived");
assert.sameValue(keysValues[1], 2, "keys branch: the value the iterator did not name is kept");
