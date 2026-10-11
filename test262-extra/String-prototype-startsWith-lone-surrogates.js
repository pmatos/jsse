// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-string.prototype.startswith
description: >
  String.prototype.startsWith compares UTF-16 code units exactly, so a lone
  surrogate only matches the identical code unit: it never matches a different
  lone surrogate or U+FFFD, and it does match one half of a surrogate pair.
  Expected values are cross-checked with Node.
info: |
  22.1.3.24 String.prototype.startsWith ( searchString [ , position ] )
    2. Let S be ? ToString(O).
    5. Let searchStr be ? ToString(searchString).
    6. Let len be the length of S.
    7. If position is undefined, let pos be 0; else let pos be
       ? ToIntegerOrInfinity(position).
    8. Let start be the result of clamping pos between 0 and len.
    9. Let searchLength be the length of searchStr.
    10. If searchLength = 0, return true.
    11. Let end be start + searchLength.
    12. If end > len, return false.
    13. Let substring be the substring of S from start to end.
    14. If substring is searchStr, return true.
    15. Return false.
---*/

assert.sameValue("\ud800x".startsWith("\udc00"), false, "different lone surrogates do not match");
assert.sameValue("\ud800x".startsWith("\ufffd"), false, "a lone surrogate is not U+FFFD");
assert.sameValue("\ufffdx".startsWith("\ud800"), false, "U+FFFD is not a lone surrogate");
assert.sameValue("\ud800x".startsWith("\ud800"), true, "identical lone surrogate matches");
assert.sameValue("x\udc00".startsWith("\ud800", 1), false, "different lone surrogate at position");
assert.sameValue("x\udc00".startsWith("\udc00", 1), true, "identical lone surrogate at position");

// Halves of a surrogate pair are individually searchable.
assert.sameValue("\ud83d\ude00".startsWith("\ud83d"), true, "leading half of a pair");
assert.sameValue("\ud83d\ude00".startsWith("\ude00", 1), true, "trailing half at position");
assert.sameValue("\ud83d\ude00".startsWith("\ude00"), false, "trailing half is not a prefix");

var searchObj = { toString: function () { return "\udc00"; } };
assert.sameValue("\ud800".startsWith(searchObj), false, "search string from toString");
assert.sameValue(new String("\udc00").startsWith(searchObj), true, "String wrapper this value");
