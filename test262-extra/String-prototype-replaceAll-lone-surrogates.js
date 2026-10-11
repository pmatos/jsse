// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-string.prototype.replaceall
description: >
  String.prototype.replaceAll with a string search value compares UTF-16 code
  units exactly, and the receiver, the replacement string, the replacer's
  results and the GetSubstitution patterns all keep lone surrogates. Expected
  values are cross-checked with Node.
info: |
  22.1.3.20 String.prototype.replaceAll ( searchValue, replaceValue )
    3. Let string be ? ToString(O).
    4. Let searchString be ? ToString(searchValue).
    ...
    10. Let position be StringIndexOf(string, searchString, 0).
    11. Repeat, while position is not not-found,
      a. Append position to matchPositions.
      b. Set position to StringIndexOf(string, searchString, position +
         advanceBy).
---*/

assert.sameValue("a\ud800b\ud800".replaceAll("\udc00", "X"), "a\ud800b\ud800", "different lone surrogate does not match");
assert.sameValue("a\ud800b\ud800".replaceAll("\ud800", "X"), "aXbX", "identical lone surrogate matches");
assert.sameValue("a\ud800b".replaceAll("\ufffd", "X"), "a\ud800b", "U+FFFD does not match a lone surrogate");
assert.sameValue("\ud83d\ude00\ud83d\ude00".replaceAll("\ude00", "X"), "\ud83dX\ud83dX", "trailing halves of pairs");
assert.sameValue("abab".replaceAll("b", "\udc00"), "a\udc00a\udc00", "replacement string keeps a lone surrogate");
assert.sameValue("abab".replaceAll("b", function () { return "\ud800"; }), "a\ud800a\ud800", "replacer result keeps a lone surrogate");
assert.sameValue("\ud800a\ud800a".replaceAll("a", "[$`]"), "\ud800[\ud800]\ud800[\ud800a\ud800]", "$` keeps lone surrogates");
assert.sameValue("\ud800".replaceAll("", "-"), "-\ud800-", "empty search value around a lone surrogate");
