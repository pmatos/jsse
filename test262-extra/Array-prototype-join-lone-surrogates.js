// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-array.prototype.join
description: >
  Array.prototype.join converts the separator and each element with ToString
  and concatenates code units, so lone surrogates survive and adjacent halves
  join into a surrogate pair. Expected values are cross-checked with Node.
info: |
  23.1.3.18 Array.prototype.join ( separator )
    4. Else, let sep be ? ToString(separator).
    ...
    7. Repeat, while k < len,
      a. If k > 0, set R to the string-concatenation of R and sep.
      ...
      c. If element is neither undefined nor null, then
        i. Let S be ? ToString(element).
        ii. Set R to the string-concatenation of R and S.
---*/

assert.sameValue(["\ud800", "\udc00"].join(""), "\ud800\udc00", "halves join into a pair");
assert.sameValue(["\ud800", "\udc00"].join("").length, 2, "joined length");
assert.sameValue(["a", "b"].join("\ud800"), "a\ud800b", "lone surrogate separator");
assert.sameValue(["\udc00", null, "\ud800"].join(), "\udc00,,\ud800", "elements keep lone surrogates");
var obj = { toString: function () { return "\ud800"; } };
assert.sameValue([obj, obj].join("-"), "\ud800-\ud800", "element from toString");
