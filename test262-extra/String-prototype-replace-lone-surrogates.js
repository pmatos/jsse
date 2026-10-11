// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-string.prototype.replace
description: >
  String.prototype.replace with a string search value compares UTF-16 code
  units exactly, and the receiver, the replacement string, the replacer's
  result and the GetSubstitution patterns all keep lone surrogates. Expected
  values are cross-checked with Node.
info: |
  22.1.3.19 String.prototype.replace ( searchValue, replaceValue )
    3. Let string be ? ToString(O).
    4. Let searchString be ? ToString(searchValue).
    ...
    8. Let position be StringIndexOf(string, searchString, 0).
    ...
    11. If functionalReplace is true, then
      a. Let replValue be ? Call(replaceValue, undefined, « searchString,
         𝔽(position), string »).
      b. Let replacement be ? ToString(replValue).
    12. Else,
      ...
      b. Let replacement be ! GetSubstitution(searchString, string, position,
         captures, undefined, replaceValue).
---*/

assert.sameValue("a\ud800b".replace("\udc00", "X"), "a\ud800b", "different lone surrogate does not match");
assert.sameValue("a\ud800b".replace("\ud800", "X"), "aXb", "identical lone surrogate matches");
assert.sameValue("a\ud800b".replace("\ufffd", "X"), "a\ud800b", "U+FFFD does not match a lone surrogate");
assert.sameValue("\ud83d\ude00".replace("\ude00", "X"), "\ud83dX", "trailing half of a pair");
assert.sameValue("ab".replace("b", "\ud800"), "a\ud800", "replacement string keeps a lone surrogate");
assert.sameValue("ab".replace("b", function () { return "\udc00"; }), "a\udc00", "replacer result keeps a lone surrogate");
assert.sameValue("\ud800ab".replace("a", "[$`|$&|$']"), "\ud800[\ud800|a|b]b", "$` keeps a lone surrogate");
assert.sameValue("ab\udc00".replace("a", "[$']"), "[b\udc00]b\udc00", "$' keeps a lone surrogate");
assert.sameValue("x\ud800y".replace("\ud800", "[$&]"), "x[\ud800]y", "$& keeps a lone surrogate");

var seen;
"a\ud800b".replace("\ud800", function (matched, position, string) {
  seen = [matched, position, string];
  return "";
});
assert.sameValue(seen[0], "\ud800", "replacer matched");
assert.sameValue(seen[1], 1, "replacer position");
assert.sameValue(seen[2], "a\ud800b", "replacer string");
