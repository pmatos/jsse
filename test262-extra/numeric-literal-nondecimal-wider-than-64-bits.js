/*---
description: >
  A NonDecimalIntegerLiteral (0x / 0o / 0b) or LegacyOctalIntegerLiteral whose
  exact integer value does not fit in 64 bits is not a SyntaxError: the
  grammar places no upper bound on digit count, and NumericValue returns
  𝔽(MV), the Number value nearest the exact mathematical value, rounding to
  +Infinity once MV is at or above 2^1024 - 2^970. An engine that lexes these
  literals with a fixed-width (e.g. u64) integer parse wrongly rejects any
  literal whose value overflows that width.
esid: sec-literals-numeric-literals
info: |
  NumericLiteral :: NonDecimalIntegerLiteral
    1. Return the NumericValue of NonDecimalIntegerLiteral.

  NumericLiteral :: LegacyOctalIntegerLiteral
    1. Return the NumericValue of LegacyOctalIntegerLiteral.

  Static Semantics: NumericValue
    NonDecimalIntegerLiteral :: 0x HexDigits
      1. Return the NumberValue of the source text matched.

    Static Semantics: MV
      A conforming implementation must support the exact mathematical value
      denoted; there is no digit-count limit in the grammar.

  The Number Type ( 𝔽(x) )
    Rounds to the nearest representable Number, ties to even; values at or
    above 2^1024 - 2^970 produce +∞.

  Numeric Literals -- Static Semantics: Early Errors
    LegacyOctalIntegerLiteral is still an early error in strict mode code,
    independent of its value.
esid: sec-numericvalue
features: [BigInt]
---*/

// Indirect eval: per sec-performeval, indirect eval always runs as global
// code, so its strictness comes only from its own source text, never from
// the strictness of the calling script. This keeps the assertions below
// exercising sloppy-mode NumericValue regardless of whether this file's
// harness-generated "strict mode" variant wraps the whole script in
// "use strict".
var indirectEval = (0, eval);

// (1) The four literals from the bug report round instead of throwing.
assert.sameValue(0xffffffffffffffffffff, Math.pow(2, 80), "hex, source literal");
assert.sameValue(indirectEval("0xffffffffffffffffffff"), Math.pow(2, 80), "hex, eval");

var wideBinary = "0b" + "1".repeat(70);
assert.sameValue(indirectEval(wideBinary), Math.pow(2, 70), "binary, 70 ones");

assert.sameValue(0o777777777777777777777777, Math.pow(2, 72), "octal, source literal");
assert.sameValue(indirectEval("0o777777777777777777777777"), Math.pow(2, 72), "octal, eval");

assert.sameValue(
  indirectEval("0777777777777777777777777"),
  Math.pow(2, 72),
  "legacy octal, eval"
);

// (2) Round-to-nearest-even at an exact tie, and just past it.
var tie = "0x2" + "0".repeat(12) + "1" + "0".repeat(16); // = 2^117 + 2^64, exact midpoint
assert.sameValue(indirectEval(tie), Math.pow(2, 117), "tie rounds to the even mantissa");
var pastTie = "0x2" + "0".repeat(12) + "1" + "0".repeat(15) + "1"; // = 2^117 + 2^64 + 1
assert.sameValue(
  indirectEval(pastTie),
  Math.pow(2, 117) + Math.pow(2, 65),
  "value just past the tie rounds up"
);

// (3) Numeric separators inside a wide literal still work.
assert.sameValue(
  indirectEval("0x1_0000_0000_0000_0000"),
  Math.pow(2, 64),
  "wide hex with separators"
);

// (4) Overflow to Infinity once the exact value is too large for any Number.
assert.sameValue(
  indirectEval("0x" + "f".repeat(256)),
  Infinity,
  "256 hex digits overflow to +Infinity"
);
assert.sameValue(
  indirectEval("0o" + "7".repeat(342)),
  Infinity,
  "342 octal digits overflow to +Infinity"
);
assert.sameValue(
  indirectEval("0b" + "1".repeat(1024)),
  Infinity,
  "1024 binary digits overflow to +Infinity"
);
assert.sameValue(
  indirectEval("0" + "7".repeat(400)),
  Infinity,
  "400-digit legacy octal overflows to +Infinity"
);

// (5) Consistency with the already-correct StringToNumber path for the same digits.
assert.sameValue(
  0xffffffffffffffffffff,
  Number("0xffffffffffffffffffff"),
  "literal matches Number() on the same digits"
);
assert.sameValue(
  0o777777777777777777777777,
  Number("0o777777777777777777777777"),
  "octal literal matches Number() on the same digits"
);

// (6) The BigInt sibling literal is unaffected: it captures the exact integer.
assert.sameValue(
  0xffffffffffffffffffffn,
  1208925819614629174706175n,
  "BigInt literal keeps the exact value, not the rounded Number"
);

// (7) Digit-run validity is unchanged: an empty digit run is still a SyntaxError.
assert.throws(SyntaxError, function () {
  eval("0x;");
}, "empty hex digits");
assert.throws(SyntaxError, function () {
  eval("0o;");
}, "empty octal digits");
assert.throws(SyntaxError, function () {
  eval("0b;");
}, "empty binary digits");

// (8) Numeric-separator misuse in a wide literal is still a SyntaxError.
assert.throws(SyntaxError, function () {
  eval("0x1__0000000000000000;");
}, "doubled separator in a wide hex literal");
assert.throws(SyntaxError, function () {
  eval("0x10000000000000000_;");
}, "trailing separator in a wide hex literal");

// (9) A wide legacy octal literal is still a SyntaxError in strict mode: the
// value itself is unrestricted, but LegacyOctalIntegerLiteral remains an
// early error under strict mode code regardless of value.
assert.throws(SyntaxError, function () {
  eval("'use strict'; 0777777777777777777777777;");
}, "wide legacy octal literal is a SyntaxError in strict mode");
