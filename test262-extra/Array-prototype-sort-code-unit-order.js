// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-comparearrayelements
description: >
  The default sort order compares strings by UTF-16 code units, not code
  points, and keeps lone surrogates. A supplementary character (lead
  surrogate 0xD800-0xDBFF) sorts before U+FFFF. Expected values are
  cross-checked with Node.
info: |
  23.1.3.30.2 CompareArrayElements ( x, y, comparator )
    5. Let xString be ? ToString(x).
    6. Let yString be ? ToString(y).
    7. Let xSmaller be ! IsLessThan(xString, yString, true).

  7.2.12 IsLessThan ( x, y, LeftFirst )
    3. If px is a String and py is a String, then
      ...
      c. For each integer i such that 0 ≤ i < lx and 0 ≤ i < ly, in ascending order, do
        i. Let cx be the numeric value of the code unit at index i within px.
        ii. Let cy be the numeric value of the code unit at index i within py.
---*/

var sorted = ["\uffff", "\u{10000}", "\ud800"].sort();
assert.sameValue(sorted[0], "\ud800", "lone lead surrogate first");
assert.sameValue(sorted[1], "\u{10000}", "supplementary character before U+FFFF");
assert.sameValue(sorted[2], "\uffff", "U+FFFF last");

var toSorted = ["\uffff", "\ud800\udc01", "\udc00"].toSorted();
assert.sameValue(toSorted[0], "\ud800\udc01", "toSorted: pair first");
assert.sameValue(toSorted[1], "\udc00", "toSorted: lone trail surrogate");
assert.sameValue(toSorted[2], "\uffff", "toSorted: U+FFFF last");

var obj = { toString: function () { return "\udc00"; } };
var mixed = [obj, "\ud800"].sort();
assert.sameValue(mixed[0], "\ud800", "lone lead before object stringifying to lone trail");
assert.sameValue(mixed[1], obj, "object sorted by its ToString");
