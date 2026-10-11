// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-trimstring
description: >
  String.prototype.trim, trimStart and trimEnd remove only white space and
  line terminators and keep lone surrogates, including ones at the edges of
  the trimmed result. Expected values are cross-checked with Node.
info: |
  22.1.3.32.1 TrimString ( string, where )
    1. Let str be ? RequireObjectCoercible(string).
    2. Let S be ? ToString(str).
    3. If where is start, then
      a. Let T be the String value that is a copy of S with leading white
         space removed.
    ...
    The definition of white space is the union of WhiteSpace and
    LineTerminator.
---*/

assert.sameValue(" \ud800 ".trim(), "\ud800", "trim");
assert.sameValue(" \ud800 ".trimStart(), "\ud800 ", "trimStart");
assert.sameValue(" \ud800 ".trimEnd(), " \ud800", "trimEnd");
assert.sameValue("\u00a0\udc00x\ud800\u2028".trim(), "\udc00x\ud800", "edges are lone surrogates");
assert.sameValue("\ud800".trim(), "\ud800", "nothing to trim");
assert.sameValue("\ufeff\u3000".trim(), "", "only white space");
