// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-stringpaddingbuiltinsimpl
description: >
  String.prototype.padStart and padEnd keep lone surrogates in both the
  receiver and the fill string, and truncate the fill at a code unit
  boundary even inside a surrogate pair. Expected values are cross-checked
  with Node.
info: |
  22.1.3.17.1 StringPaddingBuiltinsImpl ( O, maxLength, fillString, placement )
    1. Let S be ? ToString(O).
    ...
    5. Else, set fillString to ? ToString(fillString).
    6. Return StringPad(S, intMaxLength, fillString, placement).

  22.1.3.17.2 StringPad ( S, maxLength, fillString, placement )
    ...
    6. Let truncatedStringFiller be the String value consisting of repeated
       concatenations of fillString truncated to length fillLen.
---*/

assert.sameValue("x\ud800".padEnd(4, "\udc00"), "x\ud800\udc00\udc00", "padEnd keeps lone surrogates");
assert.sameValue("x\ud800".padStart(4, "\udc00"), "\udc00\udc00x\ud800", "padStart keeps lone surrogates");
assert.sameValue("\ud800".padEnd(2), "\ud800 ", "default fill keeps the receiver");
assert.sameValue("x".padEnd(2, "\ud83d\ude00"), "x\ud83d", "fill truncated inside a pair");
assert.sameValue("x".padStart(4, "\ud83d\ude00"), "\ud83d\ude00\ud83dx", "padStart fill truncated inside a pair");

var fillObj = { toString: function () { return "\ud800"; } };
assert.sameValue("x".padEnd(3, fillObj), "x\ud800\ud800", "fill from toString");
