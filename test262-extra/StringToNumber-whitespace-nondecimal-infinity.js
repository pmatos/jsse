// §7.1.4.1 StringToNumber governs ToNumber applied to a String (Number(str),
// unary +, and arithmetic coercion). Three properties are easy to get wrong when
// an engine delegates to a host float parser (e.g. Rust's f64::from_str):
//   1. The set stripped is exactly ECMAScript StrWhiteSpace | StrLineTerminator
//      (§12): U+0085 <NEL> and U+200B <ZWSP> are NOT in it (Rust's White_Space
//      wrongly includes NEL), while U+FEFF <ZWNBSP> is.
//   2. A NonDecimalIntegerLiteral (0x / 0o / 0b) whose value exceeds 2^53 must
//      round to the nearest Number, not overflow (a naive i64 parse yields NaN).
//   3. The only Infinity token is the exact word "Infinity" (optionally signed);
//      the "inf" / "infinity" / "nan" spellings a host parser accepts are NaN.
// Expected values cross-checked with Node.
// Spec: ECMAScript, sec-stringtonumber, sec-tonumber-applied-to-the-string-type.

function assertEq(actual, expected, msg) {
  // Distinguish +0 from -0 as well as ordinary inequality.
  if (actual !== expected || 1 / actual !== 1 / expected) {
    throw new Test262Error(
      msg + ": expected " + expected + " but got " + actual
    );
  }
}

function assertNaN(actual, msg) {
  if (actual === actual) {
    throw new Test262Error(msg + ": expected NaN but got " + actual);
  }
}

var NEL = String.fromCharCode(0x85);
var ZWNBSP = String.fromCharCode(0xfeff);
var ZWSP = String.fromCharCode(0x200b);
var NBSP = String.fromCharCode(0xa0);
var LS = String.fromCharCode(0x2028);
var PS = String.fromCharCode(0x2029);
var IDSP = String.fromCharCode(0x3000);

// (1) Whitespace: exactly the ECMAScript set is trimmed; Unicode-only members
// (which Rust's char::is_whitespace would strip) are not.
assertEq(Number(NBSP + "1" + NBSP), 1, "NBSP is StrWhiteSpace");
assertEq(Number(ZWNBSP + "1"), 1, "ZWNBSP is StrWhiteSpace");
assertEq(Number(LS + "1"), 1, "LINE SEPARATOR is StrLineTerminator");
assertEq(Number(PS + "1"), 1, "PARAGRAPH SEPARATOR is StrLineTerminator");
assertEq(Number(IDSP + "1"), 1, "IDEOGRAPHIC SPACE is StrWhiteSpace");
assertEq(Number("\t\n\r 5 "), 5, "ASCII whitespace is trimmed");
assertNaN(Number(NEL + "1"), "NEL (U+0085) is not StrWhiteSpace");
assertNaN(Number(ZWSP + "1"), "ZWSP (U+200B) is not StrWhiteSpace");

// (2) Non-decimal integer literals round to the nearest Number, never overflow.
assertEq(Number("0x1F"), 31, "hex");
assertEq(Number("0o17"), 15, "octal");
assertEq(Number("0b101"), 5, "binary");
assertEq(Number("0x10000000000000000"), Math.pow(2, 64), "hex 2**64 rounds, not NaN");
assertEq(Number("0xffffffffffffffffffff"), Math.pow(2, 80), "hex 2**80-1 rounds to 2**80");
assertNaN(Number("0x"), "empty hex digits");
assertNaN(Number("0b"), "empty binary digits");
assertNaN(Number("0xG"), "invalid hex digit");
assertNaN(Number("0o8"), "invalid octal digit");
assertNaN(Number("+0x1"), "NonDecimalIntegerLiteral forbids a leading sign");
assertNaN(Number("-0x1"), "NonDecimalIntegerLiteral forbids a leading sign");

// (3) Infinity is case-sensitive and only the full word.
assertEq(Number("Infinity"), Infinity, "Infinity");
assertEq(Number("+Infinity"), Infinity, "+Infinity");
assertEq(Number("-Infinity"), -Infinity, "-Infinity");
assertNaN(Number("inf"), "inf is not Infinity");
assertNaN(Number("+inf"), "+inf is not Infinity");
assertNaN(Number("infinity"), "lowercase infinity is NaN");
assertNaN(Number("nan"), "nan is NaN");
assertNaN(Number("NaN"), "the string 'NaN' is NaN");

// The same seam backs unary plus and arithmetic coercion, not just Number().
assertNaN(+"inf", "unary plus routes through StringToNumber");
assertEq(+ZWNBSP + 1, 1, "a whitespace-only string coerces to +0");
