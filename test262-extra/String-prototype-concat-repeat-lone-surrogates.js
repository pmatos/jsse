// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-string.prototype.concat
description: >
  String.prototype.concat and String.prototype.repeat work on UTF-16 code
  units, so lone surrogates survive and two lone halves can join into a
  surrogate pair. Expected values are cross-checked with Node.
info: |
  22.1.3.5 String.prototype.concat ( ...args )
    2. Let S be ? ToString(O).
    3. Let R be S.
    4. For each element next of args, do
      a. Let nextString be ? ToString(next).
      b. Set R to the string-concatenation of R and nextString.

  22.1.3.18 String.prototype.repeat ( count )
    7. Return the String value that is made from n copies of S appended
       together.
---*/

assert.sameValue("\ud800".concat("x"), "\ud800x", "concat keeps the receiver's lone surrogate");
assert.sameValue("x".concat("\udc00", "\ud800"), "x\udc00\ud800", "concat keeps argument lone surrogates");
assert.sameValue("\ud83d".concat("\ude00"), "\ud83d\ude00", "concat joins two halves into a pair");
assert.sameValue("\ud83d".concat("\ude00").length, 2, "joined pair length");
var obj = { toString: function () { return "\udc00"; } };
assert.sameValue("\ud800".concat(obj), "\ud800\udc00", "concat argument from toString");

assert.sameValue("\ud800".repeat(3), "\ud800\ud800\ud800", "repeat keeps a lone surrogate");
assert.sameValue("\ude00\ud83d".repeat(2), "\ude00\ud83d\ude00\ud83d", "repeat forms pairs across copies");
