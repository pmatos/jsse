// Copyright (C) 2026 the jsse authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-comparearrayelements
description: >
  Without a comparefn, Array.prototype.sort orders strings by UTF-16 code
  units, so a surrogate pair (0xD800..0xDBFF lead) sorts before U+E000..U+FFFF.
info: |
  CompareArrayElements ( x, y, comparefn )
  ...
  5. Let xString be ? ToString(x).
  6. Let yString be ? ToString(y).
  7. Let xSmaller be ! IsLessThan(xString, yString, true).

  IsLessThan compares String values by their UTF-16 code units (step 3.c-f),
  not by code points or by UTF-8 bytes.
---*/

var astral = "\uD800\uDC00"; // U+10000
var bmpHigh = "\uE000";
var lastBmp = "\uFFFF";

var result = [bmpHigh, astral].sort();
assert.sameValue(result[0], astral, "surrogate pair sorts before U+E000");
assert.sameValue(result[1], bmpHigh);

result = [lastBmp, "\uDBFF\uDFFF", bmpHigh, "a", astral].sort();
assert.compareArray(result, ["a", astral, "\uDBFF\uDFFF", bmpHigh, lastBmp]);

// Lone surrogates keep their code unit value.
result = ["\uE000", "\uDC00", "\uD800"].sort();
assert.compareArray(result, ["\uD800", "\uDC00", "\uE000"]);

// Elements that are not Strings are compared by their ToString results.
var obj = { toString: function () { return "\uE000"; } };
result = [obj, astral].sort();
assert.sameValue(result[0], astral);
assert.sameValue(result[1], obj);
