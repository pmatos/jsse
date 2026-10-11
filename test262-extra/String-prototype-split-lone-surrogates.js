// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-string.prototype.split
description: >
  String.prototype.split with a string separator compares UTF-16 code units
  exactly, so a lone surrogate separator only matches the identical code unit,
  and the receiver's lone surrogates survive into the result pieces. An empty
  separator splits into single code units. Expected values are cross-checked
  with Node.
info: |
  22.1.3.23 String.prototype.split ( separator, limit )
    3. Let S be ? ToString(O).
    ...
    5. Let R be ? ToString(separator).
    ...
    10. If separatorLength = 0, then
      a. Let head be the substring of S from 0 to lim.
      b. Let codeUnits be a List consisting of the sequence of code units that
         are the elements of head.
    ...
    14. Let j be StringIndexOf(S, R, 0).
---*/

function same(actual, expected, message) {
  assert.sameValue(actual.length, expected.length, message + " (length)");
  for (var i = 0; i < expected.length; i++) {
    assert.sameValue(actual[i], expected[i], message + " [" + i + "]");
  }
}

same("a\ud800b".split("\udc00"), ["a\ud800b"], "different lone surrogate does not split");
same("a\ud800b".split("\ud800"), ["a", "b"], "identical lone surrogate splits");
same("a\ud800b".split("\ufffd"), ["a\ud800b"], "U+FFFD does not match a lone surrogate");
same("a\ufffdb".split("\ud800"), ["a\ufffdb"], "a lone surrogate does not match U+FFFD");
same("\ud800".split(""), ["\ud800"], "empty separator keeps a lone surrogate");
same("\ud83d\ude00".split(""), ["\ud83d", "\ude00"], "empty separator splits a pair into halves");
same("\ud83d\ude00x\ud83d\ude00".split("\ude00"), ["\ud83d", "x\ud83d", ""], "trailing half as separator");
same("a\ud800b\ud800c".split("\ud800", 2), ["a", "b"], "limit");

var sepObj = { toString: function () { return "\udc00"; } };
same("x\udc00y".split(sepObj), ["x", "y"], "separator from toString");
same("x\ud800y".split(sepObj), ["x\ud800y"], "separator from toString does not match");
