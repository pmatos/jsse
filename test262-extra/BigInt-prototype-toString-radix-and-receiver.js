// Copyright (C) 2026 jsse contributors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
description: >
  BigInt.prototype.toString renders every supported radix exactly (powers of
  two, decimal and the other radices, zero, negatives, multi-limb values),
  accepts both a BigInt primitive and a BigInt wrapper as `this`, and
  methods and accessors inherited from BigInt.prototype receive the primitive
  itself as `this` when strict and a wrapper object when sloppy.
esid: sec-bigint.prototype.tostring
info: |
  BigInt.prototype.toString ( [ radix ] )
    1. Let x be ? ThisBigIntValue(this value).
    ...
    5. Return BigInt::toString(x, radixMV).

  10.2.1.2 OrdinaryCallBindThis ( F, calleeContext, thisArgument )
    If F.[[ThisMode]] is strict, let thisValue be thisArgument. Otherwise,
    a primitive thisArgument is replaced by ToObject(thisArgument).
features: [BigInt]
flags: [noStrict]
---*/

var big = 0x79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798n;
var values = [
  0n, 1n, -1n, 15n, 16n, -16n, 255n, 256n,
  1n << 63n, 1n << 64n, (1n << 64n) - 1n, -(1n << 64n),
  (1n << 128n) + 5n, (1n << 200n) - 1n, -(1n << 255n),
  123456789012345678901234567890n, big, -big
];

function parseBack(text, radix) {
  var negative = text[0] === "-";
  if (negative) text = text.slice(1);
  var acc = 0n;
  for (var i = 0; i < text.length; i++) {
    acc = acc * BigInt(radix) + BigInt(parseInt(text[i], 36));
  }
  return negative ? -acc : acc;
}

values.forEach(function (value) {
  for (var radix = 2; radix <= 36; radix++) {
    var text = value.toString(radix);
    assert.sameValue(parseBack(text, radix), value, value + " in radix " + radix);
    var digits = text[0] === "-" ? text.slice(1) : text;
    assert.sameValue(
      digits.length > 1 && digits[0] === "0", false,
      "no leading zero: " + value + " in radix " + radix
    );
    assert.sameValue(
      /^-?[0-9a-z]+$/.test(text), true, "lowercase digits: " + text
    );
  }
});

assert.sameValue(0n.toString(2), "0", "zero, radix 2");
assert.sameValue((-255n).toString(16), "-ff", "negative, radix 16");
assert.sameValue((2n ** 64n).toString(32), "g000000000000", "radix 32");
assert.sameValue((2n ** 64n).toString(), "18446744073709551616", "default radix");
assert.sameValue(big.toString(16), "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798");
assert.sameValue(big.toString(10), "55066263022277343669578718895168534326250603453777594175500187360389116729240");
assert.sameValue(big.toString(undefined), big.toString(10), "undefined radix");
assert.sameValue(big.toString(16.9), big.toString(16), "fractional radix truncates");

assert.sameValue(Object(255n).toString(16), "ff", "wrapper receiver");
assert.sameValue(BigInt.prototype.toString.call(Object(-8n), 2), "-1000", "call on wrapper");
assert.sameValue(BigInt.prototype.valueOf.call(Object(7n)), 7n, "valueOf on wrapper");
assert.sameValue(BigInt.prototype.valueOf.call(7n), 7n, "valueOf on primitive");

assert.throws(RangeError, function () { 1n.toString(1); }, "radix 1");
assert.throws(RangeError, function () { 1n.toString(37); }, "radix 37");
assert.throws(RangeError, function () { 1n.toString(NaN); }, "radix NaN");
assert.throws(TypeError, function () { BigInt.prototype.toString.call(1); }, "number receiver");
assert.throws(TypeError, function () { BigInt.prototype.toString.call({}); }, "plain object receiver");

var strictThis, sloppyThis;
Object.defineProperty(BigInt.prototype, "toString934StrictGetter", {
  get: function () { "use strict"; return this; },
  configurable: true
});
Object.defineProperty(BigInt.prototype, "toString934SloppyGetter", {
  get: function () { return this; },
  configurable: true
});
BigInt.prototype.toString934StrictMethod = function () { "use strict"; return this; };
BigInt.prototype.toString934SloppyMethod = function () { return this; };

var key = "toString934StrictGetter";
var sloppyKey = "toString934SloppyGetter";
for (var i = 0; i < 3; i++) {
  var n = BigInt(i + 10);
  assert.sameValue(n.toString934StrictGetter, n, "strict getter, dotted");
  assert.sameValue(n[key], n, "strict getter, computed");
  assert.sameValue(n.toString934StrictMethod(), n, "strict method");
  assert.sameValue(typeof n.toString934SloppyGetter, "object", "sloppy getter, dotted");
  assert.sameValue(typeof n[sloppyKey], "object", "sloppy getter, computed");
  assert.sameValue(typeof n.toString934SloppyMethod(), "object", "sloppy method");
  assert.sameValue(n.toString934SloppyMethod().valueOf(), n, "sloppy wrapper value");
}

delete BigInt.prototype.toString934StrictGetter;
delete BigInt.prototype.toString934SloppyGetter;
delete BigInt.prototype.toString934StrictMethod;
delete BigInt.prototype.toString934SloppyMethod;
