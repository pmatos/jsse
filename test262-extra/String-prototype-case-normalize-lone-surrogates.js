// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-string.prototype.tolowercase
description: >
  toLowerCase, toUpperCase, toLocaleLowerCase, toLocaleUpperCase and normalize
  interpret the string as code points, where a lone surrogate is a code point
  of its own with no case mapping or decomposition, so it is kept. A lone
  surrogate is neither cased nor case-ignorable, which decides Final_Sigma.
  Expected values are cross-checked with Node.
info: |
  22.1.3.28 String.prototype.toLowerCase ( )
    3. Let sText be StringToCodePoints(S).
    4. Let lowerText be toLowercase(sText) according to the Unicode Default
       Case Conversion algorithm.
    5. Let L be CodePointsToString(lowerText).

  22.1.3.15 String.prototype.normalize ( [ form ] )
    ...
    6. Let ns be the String value that is the result of normalizing S into the
       normalization form named by f as specified in the latest Unicode
       Standard, Normalization Forms.
---*/

assert.sameValue("A\ud800".toLowerCase(), "a\ud800", "toLowerCase");
assert.sameValue("a\udc00".toUpperCase(), "A\udc00", "toUpperCase");
assert.sameValue("A\ud800".toLocaleLowerCase(), "a\ud800", "toLocaleLowerCase");
assert.sameValue("a\udc00".toLocaleUpperCase(), "A\udc00", "toLocaleUpperCase");
assert.sameValue("\ud800\u00df".toUpperCase(), "\ud800SS", "special casing next to a lone surrogate");
assert.sameValue("A\u03a3\ud800".toLowerCase(), "a\u03c2\ud800", "Final_Sigma before a lone surrogate");
assert.sameValue("A\ud800\u03a3 b".toLowerCase(), "a\ud800\u03c3 b", "no Final_Sigma after a lone surrogate");

assert.sameValue("a\ud800".normalize(), "a\ud800", "NFC");
assert.sameValue("a\ud800".normalize("NFD"), "a\ud800", "NFD");
assert.sameValue("a\ud800".normalize("NFKC"), "a\ud800", "NFKC");
assert.sameValue("a\ud800".normalize("NFKD"), "a\ud800", "NFKD");
assert.sameValue("e\u0301\ud800".normalize("NFC"), "\u00e9\ud800", "NFC composes before a lone surrogate");
assert.sameValue("\u00e9\udc00".normalize("NFD"), "e\u0301\udc00", "NFD decomposes before a lone surrogate");
