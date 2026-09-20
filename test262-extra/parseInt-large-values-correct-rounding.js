/*---
description: >
  parseInt rounds the exact integer value of its digit prefix to a Number once,
  so values above 2^53 are not off by accumulated per-digit rounding errors.
esid: sec-parseint-string-radix
info: |
  parseInt ( string, radix )
    14. Let mathInt be the integer value that is represented by Z in radix-R
        notation, using the letters A-Z and a-z for digits with values 10
        through 35. However, if R is 10 and Z contains more than 20 significant
        digits, every significant digit after the 20th may be replaced by a 0
        digit, at the option of the implementation; and if R is not one of 2, 4,
        8, 10, 16, or 32, then mathInt may be an implementation-approximated
        integer representing the integer value denoted by Z in radix-R notation.
    16. Return 𝔽(sign × mathInt).

  So for R in {2, 4, 8, 16, 32}, and for R = 10 with at most 20 significant
  digits, the result must be the Number nearest to the exact value (§6.1.6.1:
  ties to the even significand, and 2^1024 is +∞). An engine that accumulates
  result = result * R + digit in a double rounds at every step and lands one or
  more ULPs off once the value exceeds 2^53. Expected values cross-checked with
  Node.
---*/

var TagTypeNumber = 0xffff000000000000;

// Decimal (R = 10), at most 20 significant digits: exact rounding is mandatory.
assert.sameValue(parseInt("18446462598732840000"), TagTypeNumber, "20-digit decimal");
assert.sameValue(
  Number.parseInt("18446462598732840000"),
  TagTypeNumber,
  "Number.parseInt 20-digit decimal"
);
assert.sameValue(
  parseInt("18446462598732840960"),
  TagTypeNumber,
  "exact decimal of 0xffff000000000000"
);
assert.sameValue(
  parseInt(String(TagTypeNumber)),
  TagTypeNumber,
  "parseInt(String(x)) round-trips a large integral Number"
);
assert.sameValue(
  parseInt("-18446462598732840000"),
  -TagTypeNumber,
  "negative 20-digit decimal"
);
assert.sameValue(parseInt("9007199254740993"), 9007199254740992, "2^53+1 ties to even (down)");
assert.sameValue(parseInt("9007199254740995"), 9007199254740996, "2^53+3 ties to even (up)");
assert.sameValue(parseInt("12345678901234567890"), 12345678901234567000, "another 20-digit decimal");
assert.sameValue(parseInt("99999999999999999999"), 1e20, "twenty nines");
assert.sameValue(parseInt("18446744073709551615"), 18446744073709552000, "2^64 - 1");
assert.sameValue(
  parseInt("18446744073709551615", 10),
  18446744073709552000,
  "2^64 - 1, explicit radix 10"
);

// Power-of-two radices: exact even where the double accumulator rounds more than once.
assert.sameValue(parseInt("6dd56642c75d9a86c7ac16", 16), 1.3278066477865898e+26, "hex, 22 digits");
assert.sameValue(parseInt("6ff4bb33ab35ea185de9", 16), 5.2869717446607274e+23, "hex, 20 digits");
assert.sameValue(parseInt("5402715477647266054", 8), 99181283151539250, "octal, 19 digits");
assert.sameValue(parseInt("uhf7ljfsnp27idt8", 32), 1.1540002647698592e+24, "radix 32, 16 digits");
assert.sameValue(parseInt("ffff000000000000", 16), TagTypeNumber, "hex 0xffff000000000000");
assert.sameValue(parseInt("ffffffffffffffff", 16), 18446744073709552000, "hex 64-bit all ones");
assert.sameValue(parseInt("fffe000000000000", 16), 0xfffe000000000000, "hex 0xfffe000000000000");
assert.sameValue(parseInt("20000000000001", 16), 9007199254740992, "hex 2^53+1 ties to even (down)");
assert.sameValue(parseInt("20000000000003", 16), 9007199254740996, "hex 2^53+3 ties to even (up)");
assert.sameValue(
  parseInt("1000000000000000000000000000000000000000000000000000011000000000000", 2),
  73786976294838220000,
  "binary value just above a rounding tie (sticky bit)"
);
assert.sameValue(parseInt("1777777777777777777777", 8), 18446744073709552000, "octal 2^64 - 1");
assert.sameValue(parseInt("fvvvvvvvvvvvv", 32), 18446744073709552000, "radix 32 all-ones");
assert.sameValue(
  parseInt("1".repeat(53) + "0".repeat(971), 2),
  Number.MAX_VALUE,
  "MAX_VALUE in binary"
);
assert.sameValue(
  parseInt("1".repeat(54) + "0".repeat(970), 2),
  Infinity,
  "2^1024 - 2^970 ties to 2^1024, which is +Infinity"
);
assert.sameValue(parseInt("1" + "0".repeat(1023), 2), Math.pow(2, 1023), "2^1023 in binary");
assert.sameValue(parseInt("1" + "0".repeat(1024), 2), Infinity, "2^1024 in binary");
assert.sameValue(parseInt("9".repeat(400)), Infinity, "400 nines overflow");

// Leading zeros do not count as significant digits; a zero magnitude keeps its sign.
assert.sameValue(parseInt("0".repeat(2000) + "1"), 1, "leading zeros");
assert.sameValue(parseInt("0".repeat(2000) + "1", 2), 1, "leading zeros, binary");
assert.sameValue(parseInt("-" + "0".repeat(30)), -0, "negative zero");
assert.sameValue(parseInt("-" + "0".repeat(2000), 2), -0, "negative zero, long, binary");
assert.sameValue(parseInt("-0"), -0, "-0");

// Z is the longest prefix of ASCII radix-R digits; other code points end it.
assert.sameValue(parseInt("18446462598732840000 tail"), TagTypeNumber, "stops at a space");
assert.sameValue(parseInt("12z", 30), 32, "stops at a digit >= R");
assert.sameValue(parseInt("12٣"), 12, "stops at ARABIC-INDIC DIGIT THREE");
assert.sameValue(parseInt("7１"), 7, "stops at FULLWIDTH DIGIT ONE");
assert.sameValue(parseInt("٣"), NaN, "no ASCII digit, NaN");
