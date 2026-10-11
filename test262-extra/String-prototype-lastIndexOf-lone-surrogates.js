// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-string.prototype.lastindexof
description: >
  String.prototype.lastIndexOf compares UTF-16 code units exactly, so a lone
  surrogate only matches the identical code unit: it never matches a different
  lone surrogate or U+FFFD, and it does match one half of a surrogate pair.
  Expected values are cross-checked with Node.
info: |
  22.1.3.11 String.prototype.lastIndexOf ( searchString [ , position ] )
    2. Let S be ? ToString(O).
    3. Let searchStr be ? ToString(searchString).
    4. Let numPos be ? ToNumber(position).
    5. Assert: If position is undefined, then numPos is NaN.
    6. If numPos is NaN, let pos be +∞; otherwise, let pos be
       ! ToIntegerOrInfinity(numPos).
    7. Let len be the length of S.
    8. Let searchLen be the length of searchStr.
    9. Let start be the result of clamping pos between 0 and len - searchLen.
    10. Let result be StringLastIndexOf(S, searchStr, start).
---*/

assert.sameValue("a\ud800".lastIndexOf("\udc00"), -1, "different lone surrogates do not match");
assert.sameValue("\ud800\udc00".lastIndexOf("\udc00\ud800"), -1, "reordered pair does not match");
assert.sameValue("\ud800a\ud800".lastIndexOf("\ud800"), 2, "identical lone surrogate matches last");
assert.sameValue("\ud800a\ud800".lastIndexOf("\ud800", 1), 0, "position limits the search");
assert.sameValue("\ud800".lastIndexOf("\ufffd"), -1, "a lone surrogate is not U+FFFD");
assert.sameValue("\ufffd".lastIndexOf("\udfff"), -1, "U+FFFD is not a lone surrogate");

// Halves of a surrogate pair are individually searchable.
assert.sameValue("\ud83d\ude00\ud83d\ude00".lastIndexOf("\ud83d"), 2, "last leading half");
assert.sameValue("\ud83d\ude00\ud83d\ude00".lastIndexOf("\ude00", 2), 1, "trailing half before position");
assert.sameValue("\ud83d\ude00".lastIndexOf("\ude00", 0), -1, "position before the trailing half");

var searchObj = { toString: function () { return "\udc00"; } };
assert.sameValue("\ud800\ud800".lastIndexOf(searchObj), -1, "search string from toString");
assert.sameValue(new String("\udc00\ud800").lastIndexOf(searchObj), 0, "String wrapper this value");
