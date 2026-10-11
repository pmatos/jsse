// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-string.prototype.indexof
description: >
  String.prototype.indexOf compares UTF-16 code units exactly, so a lone
  surrogate only matches the identical code unit: it never matches a different
  lone surrogate or U+FFFD, and it does match one half of a surrogate pair.
  Strings produced by ToString of an object (for both the this value and the
  search string) keep their lone surrogates. Expected values are cross-checked
  with Node.
info: |
  22.1.3.9 String.prototype.indexOf ( searchString [ , position ] )
    1. Let O be ? RequireObjectCoercible(this value).
    2. Let S be ? ToString(O).
    3. Let searchStr be ? ToString(searchString).
    4. Let pos be ? ToIntegerOrInfinity(position).
    5. Assert: If position is undefined, then pos is 0.
    6. Let len be the length of S.
    7. Let start be the result of clamping pos between 0 and len.
    8. Let result be StringIndexOf(S, searchStr, start).

  6.1.4.1 StringIndexOf ( string, searchValue, fromIndex )
    ... 4. For each integer i such that fromIndex ≤ i ≤ len - searchLen, in
    ascending order, do
      a. Let candidate be the substring of string from i to i + searchLen.
      b. If candidate is searchValue, return i.
---*/

assert.sameValue("\ud800".indexOf("\udc00"), -1, "different lone surrogates do not match");
assert.sameValue("\udc00".indexOf("\ud800"), -1, "different lone surrogates do not match (reversed)");
assert.sameValue("\ud800".indexOf("\ud800"), 0, "identical lone surrogate matches");
assert.sameValue("\ud800".indexOf("�"), -1, "a lone surrogate is not U+FFFD");
assert.sameValue("�".indexOf("\ud800"), -1, "U+FFFD is not a lone surrogate");
assert.sameValue("a\ud800b\udc00".indexOf("\udc00"), 3, "lone trail surrogate found at its own index");
assert.sameValue("a\ud800b\udc00".indexOf("b\udc00"), 2, "substring ending in a lone surrogate");

// Halves of a surrogate pair are individually searchable.
assert.sameValue("😀".indexOf("\ud83d"), 0, "leading half of a pair");
assert.sameValue("😀".indexOf("\ude00"), 1, "trailing half of a pair");
assert.sameValue("x😀".indexOf("\ude00", 2), 2, "trailing half at position");
assert.sameValue("😀".indexOf("\ude00", 2), -1, "position past the trailing half");
assert.sameValue("\ud800\ud800".indexOf("\ud800", 1), 1, "position skips the first lone surrogate");

// ToString of objects keeps lone surrogates.
var searchObj = { toString: function () { return "\udc00"; } };
assert.sameValue("\ud800".indexOf(searchObj), -1, "search string from toString");
assert.sameValue("x\udc00".indexOf(searchObj), 1, "search string from toString matches");
var thisObj = { toString: function () { return "\ud800"; } };
assert.sameValue(String.prototype.indexOf.call(thisObj, "\udc00"), -1, "this value from toString");
assert.sameValue(new String("\ud800").indexOf("\udc00"), -1, "String wrapper this value");
