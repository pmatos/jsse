// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-string.prototype.includes
description: >
  String.prototype.includes compares UTF-16 code units exactly, so a lone
  surrogate only matches the identical code unit: it never matches a different
  lone surrogate or U+FFFD, and it does match one half of a surrogate pair.
  Expected values are cross-checked with Node.
info: |
  22.1.3.8 String.prototype.includes ( searchString [ , position ] )
    2. Let S be ? ToString(O).
    3. Let isRegExp be ? IsRegExp(searchString).
    4. If isRegExp is true, throw a TypeError exception.
    5. Let searchStr be ? ToString(searchString).
    6. Let pos be ? ToIntegerOrInfinity(position).
    7. Assert: If position is undefined, then pos is 0.
    8. Let len be the length of S.
    9. Let start be the result of clamping pos between 0 and len.
    10. Let index be StringIndexOf(S, searchStr, start).
    11. If index is not-found, return false.
    12. Return true.
---*/

assert.sameValue("\ud800".includes("\udc00"), false, "different lone surrogates do not match");
assert.sameValue("\udbff".includes("\udfff"), false, "different lone surrogates do not match (max)");
assert.sameValue("\ud800".includes("\ufffd"), false, "a lone surrogate is not U+FFFD");
assert.sameValue("\ufffd".includes("\ud800"), false, "U+FFFD is not a lone surrogate");
assert.sameValue("ab\udc00".includes("\udc00"), true, "identical lone surrogate matches");
assert.sameValue("ab\udc00".includes("\udc00", 3), false, "position past the lone surrogate");

// Halves of a surrogate pair are individually searchable.
assert.sameValue("\ud83d\ude00".includes("\ud83d"), true, "leading half of a pair");
assert.sameValue("\ud83d\ude00".includes("\ude00", 1), true, "trailing half at position");
assert.sameValue("\ud83d\ude00".includes("\ude00", 2), false, "position past the trailing half");

var searchObj = { toString: function () { return "\udc00"; } };
assert.sameValue("\ud800".includes(searchObj), false, "search string from toString");
assert.sameValue(new String("\udc00").includes(searchObj), true, "String wrapper this value");
