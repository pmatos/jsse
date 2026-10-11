// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-string-constructor-string-value
description: >
  String(value), new String(value) and String.raw convert their arguments
  with ToString, which keeps lone surrogates. Expected values are
  cross-checked with Node.
info: |
  22.1.1.1 String ( value )
    2. Else,
      ...
      b. Let s be ? ToString(value).

  22.1.2.4 String.raw ( template, ...substitutions )
    ... ? ToString(? Get(literals, nextKey)) and ? ToString(next).
---*/

var obj = { toString: function () { return "\ud800"; } };
assert.sameValue(String(obj), "\ud800", "String(object)");
assert.sameValue(String(obj).charCodeAt(0), 0xd800, "String(object) code unit");
assert.sameValue(new String(obj).valueOf(), "\ud800", "new String(object)");
assert.sameValue(String.raw({ raw: ["\ud800", "\udc00"] }, obj), "\ud800\ud800\udc00", "String.raw");
