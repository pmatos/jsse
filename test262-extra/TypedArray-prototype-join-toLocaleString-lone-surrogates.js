// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-%typedarray%.prototype.join
description: >
  %TypedArray%.prototype.join and toLocaleString build their result from code
  units, so a lone surrogate in the separator or in an element's
  toLocaleString result survives. Expected values are cross-checked with Node.
info: |
  23.2.3.18 %TypedArray%.prototype.join ( separator )
    5. Else, let sep be ? ToString(separator).
    ...
    8. Repeat, while k < len,
      a. If k > 0, set R to the string-concatenation of R and sep.

  23.2.3.31 %TypedArray%.prototype.toLocaleString ( [ reserved1 [ , reserved2 ] ] )
    This is a distinct method that implements the same algorithm as
    Array.prototype.toLocaleString ...

  23.1.3.32 Array.prototype.toLocaleString ( [ reserved1 [ , reserved2 ] ] )
    6. Repeat, while k < len,
      ...
      c. If element is neither undefined nor null, then
        i. Let S be ? ToString(? Invoke(element, "toLocaleString")).
        ii. Set R to the string-concatenation of R and S.
features: [TypedArray]
---*/

assert.sameValue(new Uint8Array([1, 2]).join("\ud800"), "1\ud8002", "lone lead surrogate separator");
assert.sameValue(new Float64Array([1.5, 2]).join("\udc00"), "1.5\udc002", "lone trail surrogate separator");
var sep = { toString: function () { return "\udc00"; } };
assert.sameValue(new Uint8Array([1, 2]).join(sep), "1\udc002", "separator object converted losslessly");

var original = Number.prototype.toLocaleString;
Number.prototype.toLocaleString = function () { return this > 1 ? "\udc00" : "\ud800"; };
try {
  var result = new Uint8Array([1, 2]).toLocaleString();
  assert.sameValue(result, "\ud800,\udc00", "lone surrogates from toLocaleString");
  assert.sameValue(result.length, 3, "length counts each lone surrogate");
} finally {
  Number.prototype.toLocaleString = original;
}
