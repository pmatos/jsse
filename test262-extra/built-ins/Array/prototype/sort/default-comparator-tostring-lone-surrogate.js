// Copyright (C) 2026 the jsse authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-comparearrayelements
description: >
  A lone surrogate returned by an element's toString keeps its code unit
  value when the default comparator orders elements.
info: |
  CompareArrayElements ( x, y, comparefn )
  ...
  5. Let xString be ? ToString(x).
  6. Let yString be ? ToString(y).
  7. Let xSmaller be ! IsLessThan(xString, yString, true).
---*/

var lead = { toString: function () { return "\uD800"; } };
var trail = { toString: function () { return "\uDC00"; } };

var result = [lead, ""].sort();
assert.sameValue(result[0], lead, "0xD800 sorts before 0xE000");

result = ["�", trail, lead].sort();
assert.sameValue(result[0], lead);
assert.sameValue(result[1], trail);
assert.sameValue(result[2], "�");
