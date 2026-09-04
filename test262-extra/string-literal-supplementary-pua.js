/*---
esid: sec-static-semantics-sv
description: >
  String literals preserve genuine supplementary Private Use Area code points
  while eval source preserves raw lone surrogates.
info: |
  String Literals, Static Semantics: SV

  A SourceCharacter and a Unicode code point escape are encoded using
  UTF16EncodeCodePoint. A genuine U+F0000 therefore produces the surrogate
  pair 0xDB80 0xDC00, not the single code unit 0xD800.

  PerformEval parses the original String value, so a raw lone surrogate in
  eval source must remain the same lone surrogate after parsing, whether it
  appears as a bare source character or immediately after an identity
  escape (\<lone surrogate>).
---*/

function assertPlane15(value, label) {
  if (value.length !== 2) {
    throw new Test262Error(label + ": expected length 2, got " + value.length);
  }
  if (value.charCodeAt(0) !== 0xDB80 || value.charCodeAt(1) !== 0xDC00) {
    throw new Test262Error(
      label + ": expected [0xDB80, 0xDC00], got [" +
      value.charCodeAt(0) + ", " + value.charCodeAt(1) + "]"
    );
  }
  if (value.codePointAt(0) !== 0xF0000) {
    throw new Test262Error(
      label + ": expected code point 0xF0000, got " + value.codePointAt(0)
    );
  }
}

assertPlane15("\u{F0000}", "Unicode code point escape");
assertPlane15("󰀀", "raw source character");

var lone = String.fromCharCode(0xD800);
var evaluated = eval('"' + lone + '"');
if (evaluated.length !== 1 || evaluated.charCodeAt(0) !== 0xD800) {
  throw new Test262Error(
    "eval raw lone surrogate: expected [0xD800], got length " +
    evaluated.length + " and first code unit " + evaluated.charCodeAt(0)
  );
}

var identityEscaped = eval("'\\" + lone + "'");
if (identityEscaped.length !== 1 || identityEscaped.charCodeAt(0) !== 0xD800) {
  throw new Test262Error(
    "eval identity-escaped lone surrogate: expected [0xD800], got length " +
    identityEscaped.length + " and first code unit " + identityEscaped.charCodeAt(0)
  );
}
