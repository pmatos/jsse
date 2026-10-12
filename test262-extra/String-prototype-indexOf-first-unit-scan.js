// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-string.prototype.indexof
description: >
  String.prototype.indexOf, includes and lastIndexOf agree with StringIndexOf
  for partial first-unit matches, overlapping candidates, matches ending at the
  last code unit, and start positions at or beyond the end of the receiver.
info: |
  22.1.3.9 String.prototype.indexOf ( searchString [ , position ] )
    9. Let result be StringIndexOf(S, searchStr, start).
  6.1.4.1 StringIndexOf ( string, searchValue, fromIndex )
---*/

assert.sameValue("aaab".indexOf("aab"), 1, "overlapping candidate");
assert.sameValue("abababc".indexOf("ababc"), 2, "partial match then real match");
assert.sameValue("xxabc".indexOf("abc"), 2, "match ending at last unit");
assert.sameValue("xxab".indexOf("abc"), -1, "needle longer than remaining tail");
assert.sameValue("abc".indexOf("abcd"), -1, "needle longer than haystack");
assert.sameValue("abc".indexOf("c", 2), 2, "start at last unit");
assert.sameValue("abc".indexOf("c", 3), -1, "start at length, non-empty needle");
assert.sameValue("abc".indexOf("c", 100), -1, "start beyond length");
assert.sameValue("abc".indexOf("", 3), 3, "empty needle at length");
assert.sameValue("abc".indexOf("", 100), 3, "empty needle clamps to length");
assert.sameValue("abc".indexOf("a", -5), 0, "negative start clamps to zero");
assert.sameValue("abcabc".indexOf("abc", 1), 3, "second occurrence");
assert.sameValue("a".indexOf("a"), 0, "single unit");
assert.sameValue("".indexOf("a"), -1, "empty haystack");
assert.sameValue("".indexOf(""), 0, "empty both");

assert.sameValue("\ud800a𐀀".indexOf("𐀀"), 2, "pair after lone lead");
assert.sameValue("\ud800𐀀".indexOf("𐀀"), 1, "pair after repeated lead");
assert.sameValue("\ud800".indexOf("\udc00"), -1, "lone halves differ");

assert.sameValue("abcabc".includes("cab"), true, "includes across repeat");
assert.sameValue("abcabc".includes("cab", 3), false, "includes with start past match");
assert.sameValue("abcabc".lastIndexOf("abc"), 3, "lastIndexOf final occurrence");
assert.sameValue("abcabc".lastIndexOf("abc", 2), 0, "lastIndexOf with position");
