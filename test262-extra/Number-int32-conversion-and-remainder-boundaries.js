/*---
description: >
  ToInt32/ToUint32 (via `| 0` and `>>> 0`) and the Number `%` operator give
  spec results at int32, uint32, 2^53 and 2^63 boundaries, for -0, NaN and the
  infinities, and when an integer remainder is zero with a negative dividend.
esid: sec-toint32
info: |
  ToInt32 / ToUint32 reduce the truncated value modulo 2^32 (7.1.6, 7.1.7).
  Number::remainder (6.1.6.1.6): if n is -0 or d is +-Infinity, the result
  follows n; if the mathematical remainder is zero, the result has the sign of
  the dividend.
---*/

function isNegativeZero(v) {
  return v === 0 && 1 / v === -Infinity;
}

function checkInt32(input, expected) {
  var actual = input | 0;
  if (actual !== expected || isNegativeZero(actual)) {
    throw new Test262Error('(' + input + ') | 0 should be ' + expected + ', got ' + actual);
  }
}

function checkUint32(input, expected) {
  var actual = input >>> 0;
  if (actual !== expected || isNegativeZero(actual)) {
    throw new Test262Error('(' + input + ') >>> 0 should be ' + expected + ', got ' + actual);
  }
}

checkInt32(2147483647, 2147483647);
checkInt32(2147483648, -2147483648);
checkInt32(-2147483648, -2147483648);
checkInt32(-2147483649, 2147483647);
checkInt32(4294967295, -1);
checkInt32(4294967296, 0);
checkInt32(-4294967296, 0);
checkInt32(9007199254740992, 0);
checkInt32(-9007199254740992, 0);
checkInt32(9007199254740994, 2);
checkInt32(9223372036854775808, 0);
checkInt32(-9223372036854775808, 0);
checkInt32(8999999999999999000, -494666752);
checkInt32(1e21, -559939584);
checkInt32(-0, 0);
checkInt32(NaN, 0);
checkInt32(Infinity, 0);
checkInt32(-Infinity, 0);
checkInt32(-2.5, -2);
checkInt32(2.9, 2);

checkUint32(-1, 4294967295);
checkUint32(-2.5, 4294967294);
checkUint32(2147483648, 2147483648);
checkUint32(4294967295, 4294967295);
checkUint32(4294967296, 0);
checkUint32(4294967301, 5);
checkUint32(-4294967297, 4294967295);
checkUint32(9223372036854775808, 0);
checkUint32(1e21, 3735027712);
checkUint32(-0, 0);
checkUint32(NaN, 0);
checkUint32(-Infinity, 0);

function checkRemainder(n, d, expected) {
  var actual = n % d;
  var ok;
  if (expected !== expected) {
    ok = actual !== actual;
  } else if (expected === 0) {
    ok = actual === 0 && isNegativeZero(actual) === isNegativeZero(expected);
  } else {
    ok = actual === expected;
  }
  if (!ok) {
    throw new Test262Error(n + ' % ' + d + ' should be ' + (isNegativeZero(expected) ? '-0' : expected) +
      ', got ' + (isNegativeZero(actual) ? '-0' : actual));
  }
}

checkRemainder(7, 3, 1);
checkRemainder(-7, 3, -1);
checkRemainder(7, -3, 1);
checkRemainder(-7, -3, -1);
checkRemainder(6, 3, 0);
checkRemainder(-6, 3, -0);
checkRemainder(-1, 1, -0);
checkRemainder(-4, -2, -0);
checkRemainder(0, 5, 0);
checkRemainder(-0, 5, -0);
checkRemainder(-0, -5, -0);
checkRemainder(5, 0, NaN);
checkRemainder(5, -0, NaN);
checkRemainder(-2147483648, -1, -0);
checkRemainder(-2147483648, 2147483647, -1);
checkRemainder(2147483647, -2147483648, 2147483647);
checkRemainder(5.5, 2, 1.5);
checkRemainder(-5.5, 2, -1.5);
checkRemainder(5, 2.5, 0);
checkRemainder(2147483648, 3, 2);
checkRemainder(NaN, 3, NaN);
checkRemainder(3, NaN, NaN);
checkRemainder(Infinity, 3, NaN);
checkRemainder(3, Infinity, 3);
checkRemainder(-3, -Infinity, -3);
