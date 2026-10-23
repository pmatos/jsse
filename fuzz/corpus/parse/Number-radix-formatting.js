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
