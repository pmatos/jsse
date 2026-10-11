// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-array.prototype.tolocalestring
description: >
  Array.prototype.toLocaleString concatenates the code units of each element's
  toLocaleString result, so lone surrogates survive and adjacent halves join
  into a surrogate pair. Expected values are cross-checked with Node.
info: |
  23.1.3.32 Array.prototype.toLocaleString ( [ reserved1 [ , reserved2 ] ] )
    6. Repeat, while k < len,
      ...
      c. If element is neither undefined nor null, then
        i. Let S be ? ToString(? Invoke(element, "toLocaleString")).
        ii. Set R to the string-concatenation of R and S.
---*/

function el(s) { return { toLocaleString: function () { return s; } }; }

assert.sameValue([el("\ud800")].toLocaleString(), "\ud800", "lone lead surrogate");
assert.sameValue([el("\udc00"), null, el("\ud800")].toLocaleString(), "\udc00,,\ud800", "lone halves with separator");
var high = el("x\ud800");
assert.sameValue([high].toLocaleString().length, 2, "length counts the lone surrogate");
