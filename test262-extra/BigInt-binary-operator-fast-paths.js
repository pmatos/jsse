// Copyright (C) 2026 jsse contributors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
description: >
  Binary operators over two BigInts, and relational operators over a BigInt
  and a Number, give the specified results without going through ToPrimitive;
  operands that are objects, Strings or Symbols still run ToPrimitive and
  ToNumeric in left-to-right order, and the TypeError for a mixed BigInt/Number
  arithmetic operator is thrown only after both ToNumeric calls.
esid: sec-applystringornumericbinaryoperator
info: |
  13.15.3 ApplyStringOrNumericBinaryOperator ( lval, opText, rval )
    ... Let lnum be ? ToNumeric(lval). Let rnum be ? ToNumeric(rval).
    If Type(lnum) is not Type(rnum), throw a TypeError exception.

  7.2.14 IsLessThan ( x, y, LeftFirst )
    If LeftFirst is true, px = ToPrimitive(x), then py = ToPrimitive(y);
    otherwise py first. BigInt vs Number compares the mathematical values.
features: [BigInt]
includes: [compareArray.js]
---*/

var big = 0x79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798n;
var pairs = [[7n, 3n], [-7n, 3n], [3n, -7n], [0n, 0n], [-5n, -5n], [big, 12345n], [-big, big], [big, big]];
pairs.forEach(function (p) {
  var a = p[0], b = p[1];
  assert.sameValue(a + b, BigInt.asIntN(1024, a + b), "sum is a BigInt");
  assert.sameValue(typeof (a * b), "bigint", "product is a BigInt");
  assert.sameValue(a - b + b, a, "a - b + b");
  assert.sameValue((a & b) | (a ^ b), a | b, "and/or/xor identity");
  assert.sameValue(a < b, !(a >= b), "lt/ge complement");
  assert.sameValue(a > b, !(a <= b), "gt/le complement");
  assert.sameValue(a == b, a === b, "loose and strict equal agree for BigInts");
  assert.sameValue(a != b, !(a === b), "not equal");
});

assert.sameValue(7n + 3n, 10n);
assert.sameValue(7n - 10n, -3n);
assert.sameValue(-7n * 3n, -21n);
assert.sameValue(-7n / 2n, -3n, "division truncates toward zero");
assert.sameValue(-7n % 3n, -1n, "remainder takes the sign of the dividend");
assert.sameValue(2n ** 100n, 1267650600228229401496703205376n);
assert.sameValue(1n << 70n, 1180591620717411303424n);
assert.sameValue(-5n >> 1n, -3n, ">> rounds toward negative infinity");
assert.sameValue(1n << -1n, 0n, "negative left shift is a right shift");
assert.sameValue(-8n >> -1n, -16n, "negative right shift is a left shift");
assert.sameValue(-6n & 3n, 2n);
assert.sameValue(-6n | 3n, -5n);
assert.sameValue(-6n ^ 3n, -7n);

// Errors keep their types, from every evaluation route.
function check(f) {
  assert.throws(RangeError, f, "division by zero");
}
check(function () { var a = 1n, b = 0n; return a / b; });
check(function () { var a = 1n, b = 0n; return a % b; });
assert.throws(RangeError, function () { var a = 2n, b = -1n; return a ** b; }, "negative exponent");
assert.throws(TypeError, function () { var a = 2n, b = 1n; return a >>> b; }, ">>> on BigInts");
assert.throws(TypeError, function () { var a = 2n; return a + 1; }, "BigInt + Number");
assert.throws(TypeError, function () { var a = 2n; return 1 - a; }, "Number - BigInt");
assert.throws(TypeError, function () { var a = 2n; return a & 1; }, "BigInt & Number");
assert.throws(TypeError, function () { var a = 2n; return a << 1; }, "BigInt << Number");
assert.throws(TypeError, function () { var a = 2n; return a ** 2; }, "BigInt ** Number");

// Compound assignment shares the operator.
var acc = 5n;
acc += 3n; acc *= 4n; acc -= 2n; acc /= 3n; acc %= 7n; acc **= 2n;
acc <<= 3n; acc >>= 1n; acc &= 0xffn; acc |= 0x100n; acc ^= 1n;
assert.sameValue(acc, 293n, "compound assignment chain");

// A VM-compiled loop exercises the same operators repeatedly.
function loop() {
  var a = big, b = 12345n, s = 2n, r = a;
  for (var i = 0; i < 50; i++) {
    r = a + b;
    r = r * s;
    r = r & a;
    r = r >> s;
  }
  return r;
}
assert.sameValue(loop(), ((((big + 12345n) * 2n) & big) >> 2n), "loop result");

// BigInt vs Number relational comparisons are exact.
assert.sameValue(1n < 1.5, true);
assert.sameValue(2n < 1.5, false);
assert.sameValue(-1n < -0.5, true);
assert.sameValue(-1n > -1.5, true);
assert.sameValue(0n < 0, false);
assert.sameValue(0n <= -0, true);
assert.sameValue(1.5 > 1n, true);
assert.sameValue(1.5 < 2n, true);
assert.sameValue(0 >= 0n, true);
assert.sameValue(1n < NaN, false);
assert.sameValue(1n >= NaN, false);
assert.sameValue(NaN < 1n, false);
assert.sameValue(NaN <= 1n, false);
assert.sameValue(1n < Infinity, true);
assert.sameValue(1n > -Infinity, true);
assert.sameValue(Infinity > big, true);
assert.sameValue(-Infinity < -big, true);
assert.sameValue(9007199254740993n > 9007199254740992, true, "beyond 2^53");
assert.sameValue(9007199254740993n <= 9007199254740992, false, "beyond 2^53");
assert.sameValue(9223372036854775807n < 9223372036854775808, true, "around 2^63");
assert.sameValue(9223372036854775808n > 9223372036854775807, false, "2^63 as a Number is 2^63");
assert.sameValue(9223372036854775808n >= 9223372036854775808, true, "2^63 equal");
assert.sameValue(-9223372036854775808n <= -9223372036854775808, true, "-2^63 equal");
assert.sameValue(9000000000000000001n > 9e18, true, "just above the i64 comparison cutoff");
assert.sameValue(8999999999999999999n < 9e18, true, "just below the i64 comparison cutoff");
assert.sameValue(9223372036854775807n < 9.2e18, false, "i64 max against 9.2e18");
assert.sameValue(9.2e18 < 9223372036854775807n, true, "9.2e18 against i64 max");
assert.sameValue(-9000000000000000001n < -9e18, true, "negative side of the cutoff");
assert.sameValue(1n < 1e19, true, "Number beyond the i64 range");
assert.sameValue(-1n > -1e19, true, "negative Number beyond the i64 range");
assert.sameValue(big > 1e30, true);
assert.sameValue(big < 1e80, true);
assert.sameValue(-5n < 3, true);
assert.sameValue(5n > -3, true);

// ToPrimitive and ToNumeric ordering on non-BigInt operands is untouched.
var log = [];
function obj(name, value) {
  return {
    valueOf: function () { log.push(name); return value; }
  };
}

log = [];
assert.sameValue(obj("l", 4n) + obj("r", 5n), 9n);
assert.compareArray(log, ["l", "r"], "+ converts left then right");

log = [];
assert.sameValue(obj("l", 4n) * obj("r", 5n), 20n);
assert.compareArray(log, ["l", "r"], "* converts left then right");

log = [];
assert.sameValue(obj("l", 4n) < obj("r", 5n), true);
assert.compareArray(log, ["l", "r"], "< converts left then right");

log = [];
assert.sameValue(obj("l", 4n) > obj("r", 5n), false);
assert.compareArray(log, ["l", "r"], "> still converts left then right");

log = [];
assert.sameValue(obj("l", 4n) <= obj("r", 5n), true);
assert.compareArray(log, ["l", "r"], "<= still converts left then right");

log = [];
assert.sameValue(obj("l", 4n) >= obj("r", 5n), false);
assert.compareArray(log, ["l", "r"], ">= still converts left then right");

log = [];
assert.sameValue(obj("l", 4n) < 5, true);
assert.compareArray(log, ["l"], "object vs Number relational");

log = [];
assert.throws(TypeError, function () { return obj("l", 4n) - obj("r", 5); });
assert.compareArray(log, ["l", "r"], "both ToNumeric calls run before the mixed-type TypeError");

log = [];
assert.throws(TypeError, function () { return obj("l", 4n) & obj("r", 5); });
assert.compareArray(log, ["l", "r"], "& mixed-type TypeError after both conversions");

log = [];
assert.throws(TypeError, function () {
  return { valueOf: function () { log.push("l"); throw new TypeError("left"); } } +
    { valueOf: function () { log.push("r"); return 1n; } };
});
assert.compareArray(log, ["l"], "a throwing left operand stops evaluation");

// Mixed BigInt and String comparisons keep StringToBigInt semantics.
assert.sameValue(1n < "2", true);
assert.sameValue("2" > 1n, true);
assert.sameValue(1n < "x", false);
assert.sameValue(1n >= "x", false);
assert.throws(TypeError, function () { return 1n < Symbol(); });
assert.throws(TypeError, function () { return Symbol() < 1n; });
assert.sameValue(1n + "2", "12");
assert.sameValue("2" + 1n, "21");
