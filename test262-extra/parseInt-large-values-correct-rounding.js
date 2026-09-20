// §19.2.5 parseInt ( string, radix ) step 14 takes mathInt as the exact integer
// value of the digit prefix Z in radix R, and step 16 returns 𝔽(sign × mathInt),
// i.e. the Number nearest to it (§6.1.6.1: ties to the even significand, and
// 2^1024 is +∞). The only relaxations are that for R = 10 digits after the 20th
// significant one may be replaced by 0, and that R outside {2, 4, 8, 10, 16, 32}
// may be approximated. So for R in {2, 4, 8, 16, 32}, and for R = 10 with at
// most 20 significant digits, the result must be rounded exactly once.
// An engine that accumulates result = result * R + digit in a double rounds at
// every step and lands one or more ULPs off once the value exceeds 2^53.
// Expected values cross-checked with Node.
// Spec: ECMAScript, sec-parseint-string-radix, sec-ecmascript-language-types-number-type.

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

function repeat(ch, n) {
  var out = "";
  for (var i = 0; i < n; i++) out += ch;
  return out;
}

var TagTypeNumber = 0xffff000000000000;

// Decimal (R = 10), at most 20 significant digits: exact rounding is mandatory.
assertEq(parseInt("18446462598732840000"), TagTypeNumber, "20-digit decimal");
assertEq(
  Number.parseInt("18446462598732840000"),
  TagTypeNumber,
  "Number.parseInt 20-digit decimal"
);
assertEq(parseInt("18446462598732840960"), TagTypeNumber, "exact decimal of 0xffff000000000000");
assertEq(
  parseInt(String(TagTypeNumber)),
  TagTypeNumber,
  "parseInt(String(x)) round-trips a large integral Number"
);
assertEq(parseInt("-18446462598732840000"), -TagTypeNumber, "negative 20-digit decimal");
assertEq(parseInt("9007199254740993"), 9007199254740992, "2^53+1 ties to even (down)");
assertEq(parseInt("9007199254740995"), 9007199254740996, "2^53+3 ties to even (up)");
assertEq(parseInt("12345678901234567890"), 12345678901234567000, "20-digit decimal");
assertEq(parseInt("99999999999999999999"), 1e20, "twenty nines");
assertEq(parseInt("18446744073709551615"), 18446744073709552000, "2^64 - 1");
assertEq(parseInt("18446744073709551615", 10), 18446744073709552000, "2^64 - 1, explicit radix 10");

// Prefix handling upstream of the digit conversion is unchanged.
assertEq(parseInt("0x10", 10), 0, "0x is not a prefix when R is 10");
assertEq(parseInt("0x10"), 16, "0x prefix, radix omitted");
assertEq(parseInt("0x10", 16), 16, "0x prefix, radix 16");
assertEq(parseInt("0x10", 0), 16, "0x prefix, radix 0");
assertEq(parseInt("12abc"), 12, "scan stops at the first non-digit");
assertEq(parseInt("12abc", 16), 0x12abc, "hex digits a-c");
assertNaN(parseInt("abc"), "no digits");
assertNaN(parseInt(""), "empty");
assertNaN(parseInt("0x"), "prefix only");
assertNaN(parseInt("1", 37), "radix 37");
assertNaN(parseInt("1", 1), "radix 1");

// Power-of-two radices: exact even where the double accumulator rounds twice.
assertEq(parseInt("ffff000000000000", 16), TagTypeNumber, "hex 0xffff000000000000");
assertEq(parseInt("ffffffffffffffff", 16), 18446744073709552000, "hex 64-bit all ones");
assertEq(parseInt("fffe000000000000", 16), 0xfffe000000000000, "hex 0xfffe000000000000");
assertEq(parseInt("20000000000001", 16), 9007199254740992, "hex 2^53+1 ties to even (down)");
assertEq(parseInt("20000000000003", 16), 9007199254740996, "hex 2^53+3 ties to even (up)");
assertEq(
  parseInt("1000000000000000000000000000000000000000000000000000011000000000000", 2),
  73786976294838220000,
  "binary value just above a rounding tie (sticky bit)"
);
assertEq(parseInt("1777777777777777777777", 8), 18446744073709552000, "octal 2^64 - 1");
assertEq(parseInt("fvvvvvvvvvvvv", 32), 18446744073709552000, "radix 32 all-ones");
assertEq(parseInt(repeat("1", 53) + repeat("0", 971), 2), Number.MAX_VALUE, "MAX_VALUE in binary");
assertEq(
  parseInt(repeat("1", 54) + repeat("0", 970), 2),
  Infinity,
  "2^1024 - 2^970 ties to 2^1024, which is +Infinity"
);
assertEq(parseInt("1" + repeat("0", 1023), 2), Math.pow(2, 1023), "2^1023 in binary");
assertEq(parseInt("1" + repeat("0", 1024), 2), Infinity, "2^1024 in binary");
assertEq(parseInt(repeat("9", 400)), Infinity, "400 nines overflow");

// Leading zeros do not count as significant digits; a zero magnitude keeps its sign.
assertEq(parseInt(repeat("0", 2000) + "1"), 1, "leading zeros");
assertEq(parseInt(repeat("0", 2000) + "1", 2), 1, "leading zeros, binary");
assertEq(parseInt("-" + repeat("0", 30)), -0, "negative zero");
assertEq(parseInt("-" + repeat("0", 2000), 2), -0, "negative zero, long, binary");
assertEq(parseInt("-0"), -0, "-0");
