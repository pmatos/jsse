// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-parseint-string-radix
description: >
  parseInt of a Number argument parses the string form of the number, so it
  truncates inside the plain decimal range, reads the leading digit of an
  exponent form outside it, and keeps the sign of zero results.
---*/

assert.sameValue(parseInt(0), 0);
assert.sameValue(1 / parseInt(0), Infinity, "+0");
assert.sameValue(1 / parseInt(-0), Infinity, "-0 prints as 0");
assert.sameValue(parseInt(3.99), 3);
assert.sameValue(parseInt(-3.99), -3);
assert.sameValue(1 / parseInt(-0.5), -Infinity, "(-1, 0) gives -0");
assert.sameValue(1 / parseInt(0.5), Infinity);
assert.sameValue(parseInt(-1), -1);
assert.sameValue(parseInt(1), 1);
assert.sameValue(parseInt(123456789), 123456789);
assert.sameValue(parseInt(2147483648.75), 2147483648);

assert.sameValue(parseInt(0.000001), 0, "smallest plain decimal");
assert.sameValue(parseInt(0.0000001), 1, "1e-7 prints as 1e-7");
assert.sameValue(parseInt(0.00000051), 5, "5.1e-7 prints as 5.1e-7");
assert.sameValue(parseInt(-0.0000001), -1);
assert.sameValue(parseInt(5e-324), 5);
assert.sameValue(1 / parseInt(-0.000001), -Infinity, "-1e-6 prints as -0.000001");

assert.sameValue(parseInt(1e20), 100000000000000000000);
assert.sameValue(parseInt(123456789012345680000), 123456789012345680000);
assert.sameValue(parseInt(1e21), 1, "1e21 prints as 1e+21");
assert.sameValue(parseInt(1.5e21), 1);
assert.sameValue(parseInt(-1e21), -1);
assert.sameValue(parseInt(1.7976931348623157e308), 1);

assert.sameValue(parseInt(NaN), NaN);
assert.sameValue(parseInt(Infinity), NaN);
assert.sameValue(parseInt(-Infinity), NaN);

assert.sameValue(parseInt(255, 16), 0x255, "radix 16 reads the decimal string");
assert.sameValue(parseInt(10, 2), 2);
assert.sameValue(parseInt(11, 10), 11);
assert.sameValue(parseInt(11, 0), 11);
assert.sameValue(parseInt(11, undefined), 11);
assert.sameValue(parseInt(11, 37), NaN);
assert.sameValue(parseInt(11, 1), NaN);

var order = [];
var radix = {
  valueOf: function () {
    order.push("radix");
    return 10;
  }
};
assert.sameValue(parseInt(12.5, radix), 12);
assert.sameValue(order.join(), "radix", "radix is still converted");

var input = {
  toString: function () {
    order.push("input");
    return "42";
  }
};
order.length = 0;
radix.valueOf = function () {
  order.push("radix");
  return 0;
};
assert.sameValue(parseInt(input, radix), 42);
assert.sameValue(order.join(), "input,radix", "string conversion precedes radix conversion");
