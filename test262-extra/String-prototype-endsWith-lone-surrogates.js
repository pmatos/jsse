// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-string.prototype.endswith
description: >
  String.prototype.endsWith compares UTF-16 code units exactly, so a lone
  surrogate only matches the identical code unit: it never matches a different
  lone surrogate or U+FFFD, and it does match one half of a surrogate pair.
  Expected values are cross-checked with Node.
info: |
  22.1.3.7 String.prototype.endsWith ( searchString [ , endPosition ] )
    2. Let S be ? ToString(O).
    5. Let searchStr be ? ToString(searchString).
    6. Let len be the length of S.
    7. If endPosition is undefined, let pos be len; else let pos be
       ? ToIntegerOrInfinity(endPosition).
    8. Let end be the result of clamping pos between 0 and len.
    9. Let searchLength be the length of searchStr.
    10. If searchLength = 0, return true.
    11. Let start be end - searchLength.
    12. If start < 0, return false.
    13. Let substring be the substring of S from start to end.
    14. If substring is searchStr, return true.
    15. Return false.
---*/

assert.sameValue("x\ud800".endsWith("\udc00"), false, "different lone surrogates do not match");
assert.sameValue("x\ud800".endsWith("\ufffd"), false, "a lone surrogate is not U+FFFD");
assert.sameValue("x\ufffd".endsWith("\ud800"), false, "U+FFFD is not a lone surrogate");
assert.sameValue("x\ud800".endsWith("\ud800"), true, "identical lone surrogate matches");
assert.sameValue("\ud800x".endsWith("\udc00", 1), false, "different lone surrogate before endPosition");
assert.sameValue("\ud800x".endsWith("\ud800", 1), true, "identical lone surrogate before endPosition");

// Halves of a surrogate pair are individually searchable.
assert.sameValue("\ud83d\ude00".endsWith("\ude00"), true, "trailing half of a pair");
assert.sameValue("\ud83d\ude00".endsWith("\ud83d", 1), true, "leading half before endPosition");
assert.sameValue("\ud83d\ude00".endsWith("\ud83d"), false, "leading half is not a suffix");

var searchObj = { toString: function () { return "\udc00"; } };
assert.sameValue("\ud800".endsWith(searchObj), false, "search string from toString");
assert.sameValue(new String("x\udc00").endsWith(searchObj), true, "String wrapper this value");
