// Tests Number.prototype.toString with various radixes.
// Spec: ECMAScript 2024, sec-number.prototype.tostring

// Default radix 10
if ((255).toString() !== "255") {
  throw new Test262Error('(255).toString() should be "255", got: ' + (255).toString());
}

// Binary (radix 2)
if ((255).toString(2) !== "11111111") {
  throw new Test262Error('(255).toString(2) should be "11111111", got: ' + (255).toString(2));
}

// Octal (radix 8)
if ((255).toString(8) !== "377") {
  throw new Test262Error('(255).toString(8) should be "377", got: ' + (255).toString(8));
}

// Hex (radix 16)
if ((255).toString(16) !== "ff") {
  throw new Test262Error('(255).toString(16) should be "ff", got: ' + (255).toString(16));
}

// Radix 36
if ((35).toString(36) !== "z") {
  throw new Test262Error('(35).toString(36) should be "z", got: ' + (35).toString(36));
}

// Negative numbers
if ((-255).toString(16) !== "-ff") {
  throw new Test262Error('(-255).toString(16) should be "-ff", got: ' + (-255).toString(16));
}

// Zero
if ((0).toString(2) !== "0") {
  throw new Test262Error('(0).toString(2) should be "0", got: ' + (0).toString(2));
}

// Special values
if (NaN.toString() !== "NaN") {
  throw new Test262Error('NaN.toString() should be "NaN"');
}
if (Infinity.toString() !== "Infinity") {
  throw new Test262Error('Infinity.toString() should be "Infinity"');
}
if ((-Infinity).toString() !== "-Infinity") {
  throw new Test262Error('(-Infinity).toString() should be "-Infinity"');
}

// toExponential edge cases
if ((0).toExponential() !== "0e+0") {
  throw new Test262Error('(0).toExponential() should be "0e+0", got: ' + (0).toExponential());
}
if ((123.456).toExponential(2) !== "1.23e+2") {
  throw new Test262Error('(123.456).toExponential(2) should be "1.23e+2", got: ' + (123.456).toExponential(2));
}
if ((0.001).toExponential(1) !== "1.0e-3") {
  throw new Test262Error('(0.001).toExponential(1) should be "1.0e-3", got: ' + (0.001).toExponential(1));
}

// toExponential with NaN/Infinity
if (NaN.toExponential() !== "NaN") {
  throw new Test262Error('NaN.toExponential() should be "NaN"');
}
if (Infinity.toExponential() !== "Infinity") {
  throw new Test262Error('Infinity.toExponential() should be "Infinity"');
}

// toPrecision edge cases
// 5.55 is actually 5.5499999... in IEEE 754, so toPrecision(2) = "5.5"
if ((5.55).toPrecision(2) !== "5.5") {
  throw new Test262Error('(5.55).toPrecision(2) should be "5.5", got: ' + (5.55).toPrecision(2));
}
if ((0.000123).toPrecision(2) !== "0.00012") {
  throw new Test262Error('(0.000123).toPrecision(2) should be "0.00012", got: ' + (0.000123).toPrecision(2));
}
if ((123456).toPrecision(4) !== "1.235e+5") {
  throw new Test262Error('(123456).toPrecision(4) should be "1.235e+5", got: ' + (123456).toPrecision(4));
}

// Test all radixes 2-36 produce valid output
for (var r = 2; r <= 36; r++) {
  var result = (100).toString(r);
  if (typeof result !== 'string' || result.length === 0) {
    throw new Test262Error('(100).toString(' + r + ') should produce a non-empty string');
  }
  // Verify round-trip
  var parsed = parseInt(result, r);
  if (parsed !== 100) {
    throw new Test262Error('parseInt(' + JSON.stringify(result) + ', ' + r + ') should be 100, got: ' + parsed);
  }
}

// Values >= 2^63 must format exactly, not saturate through i64 (issue #678)
if ((0xffff000000000000).toString(16) !== "ffff000000000000") {
  throw new Test262Error('(0xffff000000000000).toString(16) should be "ffff000000000000", got: ' + (0xffff000000000000).toString(16));
}

// Power-of-two radixes have no digit ambiguity: each digit is a fixed bit-group
// of the exact value, so the exact expansion is provably correct at the 2^63/2^64
// boundaries and for Number.MAX_VALUE.
if ((2 ** 63).toString(16) !== "8000000000000000") {
  throw new Test262Error('(2 ** 63).toString(16) should be "8000000000000000", got: ' + (2 ** 63).toString(16));
}
if ((2 ** 63).toString(2) !== "1" + "0".repeat(63)) {
  throw new Test262Error('(2 ** 63).toString(2) mismatch, got: ' + (2 ** 63).toString(2));
}
if ((2 ** 64).toString(16) !== "10000000000000000") {
  throw new Test262Error('(2 ** 64).toString(16) should be "10000000000000000", got: ' + (2 ** 64).toString(16));
}
if ((2 ** 64).toString(2) !== "1" + "0".repeat(64)) {
  throw new Test262Error('(2 ** 64).toString(2) mismatch, got: ' + (2 ** 64).toString(2));
}

var maxValueHex = Number.MAX_VALUE.toString(16);
if (maxValueHex.length !== 256) {
  throw new Test262Error('Number.MAX_VALUE.toString(16) should have length 256, got length: ' + maxValueHex.length);
}
if (BigInt("0x" + maxValueHex) !== BigInt(Number.MAX_VALUE)) {
  throw new Test262Error('Number.MAX_VALUE.toString(16) does not round-trip through BigInt');
}
var maxValueBin = Number.MAX_VALUE.toString(2);
if (maxValueBin.length !== 1024) {
  throw new Test262Error('Number.MAX_VALUE.toString(2) should have length 1024, got length: ' + maxValueBin.length);
}
if (BigInt("0b" + maxValueBin) !== BigInt(Number.MAX_VALUE)) {
  throw new Test262Error('Number.MAX_VALUE.toString(2) does not round-trip through BigInt');
}

// Radix 36 is not a power of two, so once a magnitude's ULP exceeds 1 (true for
// everything >= 2^53) multiple digit strings can validly satisfy the spec's
// "Number::toString" step 5 (sec-numeric-types-number-tostring) reconstruction
// criterion. So this asserts a round-trip property instead of a specific
// hardcoded string. (Verified empirically that Node's own radix-36 output for
// 0xffff000000000000 does *not* round-trip to the original value, so Node's
// output must not be used as ground truth here — see PR description.)
function decodeBase36(s) {
  var negative = s[0] === '-';
  var digits = negative ? s.slice(1) : s;
  var value = 0n;
  for (var i = 0; i < digits.length; i++) {
    value = value * 36n + BigInt(parseInt(digits[i], 36));
  }
  return negative ? -value : value;
}

[2 ** 63, 2 ** 64, Number.MAX_VALUE].forEach(function (original) {
  var s = original.toString(36);
  var decoded = decodeBase36(s);
  if (Number(decoded) !== original) {
    throw new Test262Error(
      original + '.toString(36) = "' + s + '" does not round-trip: Number(decoded) = ' + Number(decoded)
    );
  }
});

// Invalid radix should throw RangeError
var invalidRadixes = [0, 1, 37, -1, 100];
for (var i = 0; i < invalidRadixes.length; i++) {
  try {
    (0).toString(invalidRadixes[i]);
    throw new Test262Error('toString(' + invalidRadixes[i] + ') should throw RangeError');
  } catch (e) {
    if (!(e instanceof RangeError)) {
      throw new Test262Error('toString(' + invalidRadixes[i] + ') should throw RangeError, got: ' + e);
    }
  }
}
