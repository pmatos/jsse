// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-string.prototype-%symbol.iterator%
description: >
  String.prototype[Symbol.iterator] converts a non-string receiver with
  ToString and keeps its lone surrogates. Expected values are cross-checked
  with Node.
info: |
  22.1.3.36 String.prototype [ %Symbol.iterator% ] ( )
    1. Let string be the this value.
    2. Perform ? RequireObjectCoercible(string).
    3. Set string to ? ToString(string).
features: [Symbol.iterator]
---*/

var obj = { toString: function () { return "a\ud800\udc00\udc00"; } };
var parts = [];
var iter = String.prototype[Symbol.iterator].call(obj);
for (var step = iter.next(); !step.done; step = iter.next()) {
  parts.push(step.value);
}
assert.sameValue(parts.length, 3, "three code points");
assert.sameValue(parts[0], "a", "first");
assert.sameValue(parts[1], "\ud800\udc00", "surrogate pair");
assert.sameValue(parts[2], "\udc00", "lone trailing surrogate");
