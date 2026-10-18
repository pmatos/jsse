// Copyright (C) 2026 jsse contributors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
description: >
  Arithmetic, bitwise, shift and relational operators over two BigInt operands
  inside compiled function bodies, including comparisons that feed a
  conditional jump and RangeErrors raised mid-expression that are caught by a
  handler in the same function, give the specified results and leave the
  surrounding evaluation intact.
esid: sec-applystringornumericbinaryoperator
info: |
  13.15.3 ApplyStringOrNumericBinaryOperator ( lval, opText, rval )
    If lnum is a BigInt, apply the BigInt operation named by opText; a zero
    divisor in BigInt::divide and BigInt::remainder and a negative exponent in
    BigInt::exponentiate throw a RangeError.

  7.2.14 IsLessThan ( x, y, LeftFirst )
    If both are BigInts, return BigInt::lessThan(px, py).
features: [BigInt]
---*/

var big = 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffc2fn;
var other = 0x79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798n;

function jumps(a, b) {
  var out = "";
  if (a < b) out += "lt "; else out += "ge ";
  if (a > b) out += "gt "; else out += "le ";
  if (a <= b) out += "lte "; else out += "gt2 ";
  if (a >= b) out += "gte"; else out += "lt2";
  return out;
}
assert.sameValue(jumps(1n, 2n), "lt le lte lt2");
assert.sameValue(jumps(2n, 1n), "ge gt gt2 gte");
assert.sameValue(jumps(3n, 3n), "ge le lte gte");
assert.sameValue(jumps(-big, big), "lt le lte lt2");
assert.sameValue(jumps(big, -big), "ge gt gt2 gte");
assert.sameValue(jumps(-big, -other), "lt le lte lt2");
assert.sameValue(jumps(0n, -0n), "ge le lte gte");

function whileLoop(limit) {
  var i = 0n, n = 0;
  while (i < limit) { i += 1n; n++; }
  return n;
}
assert.sameValue(whileLoop(25n), 25);
assert.sameValue(whileLoop(-5n), 0);

function mixedJump(a) {
  var out = [];
  if (a < 2) out.push("lt"); else out.push("ge");
  if (a >= 2n) out.push("ge"); else out.push("lt");
  return out.join();
}
assert.sameValue(mixedJump(1n), "lt,lt", "BigInt against Number and BigInt in one body");
assert.sameValue(mixedJump(2n), "ge,ge");

function ops(a, b) {
  return [a + b, a - b, a * b, a / b, a % b, a & b, a | b, a ^ b, a << 3n, a >> 3n, a ** 2n, -a, ~a];
}
function expected(a, b) {
  return [
    BigInt.asIntN(2048, a + b), BigInt.asIntN(2048, a - b), BigInt.asIntN(4096, a * b),
    (a - (a % b)) / b, a - ((a - (a % b)) / b) * b,
    a & b, a | b, a ^ b, a * 8n, (a - (((a % 8n) + 8n) % 8n)) / 8n, a * a, 0n - a, -a - 1n
  ];
}
[[big, other], [other, big], [-big, other], [big, -other], [-other, -big], [7n, 3n], [-7n, 3n], [7n, -3n]]
  .forEach(function (p) {
    var got = ops(p[0], p[1]), want = expected(p[0], p[1]);
    for (var i = 0; i < want.length; i++) {
      assert.sameValue(got[i], want[i], "operator " + i + " on " + p[0] + ", " + p[1]);
    }
  });

// Known values (computed with an independent engine), through compiled operator sites.
function literals(a, b, m) {
  return [
    a * b,
    (a * b) % m,
    (b * b) % a,
    a / b,
    a % b,
    a & b,
    a | b,
    a ^ b,
    b - a,
    -a % b,
    a / -b
  ];
}
var lit = literals(big, other, big);
assert.sameValue(lit[0], 0x79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815a9d39af48667e19bc16f760b76860b73fcc02ed63dce1d6acd6439924a2883bc5572df4e8n);
assert.sameValue(lit[1], 0n);
assert.sameValue(lit[2], 0x8550e7d238fcf3086ba9adcf0fb52a9de3652194d06cb5bb38d50229b854fc49n);
assert.sameValue(lit[3], 2n);
assert.sameValue(lit[4], 0xc8333020c4688a754bf3ad462f1e9f1fac80649a463ae4d4c1afd48d20fccffn);
assert.sameValue(lit[5], 0x79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815a16f81408n);
assert.sameValue(lit[6], 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffbfn);
assert.sameValue(lit[7], 0x8641998106234453aa5f9d6a3178f4f8fd640324d231d726a60d7ea5e907ebb7n);
assert.sameValue(lit[8], -0x8641998106234453aa5f9d6a3178f4f8fd640324d231d726a60d7ea3e907e497n);
assert.sameValue(lit[9], -0xc8333020c4688a754bf3ad462f1e9f1fac80649a463ae4d4c1afd48d20fccffn);
assert.sameValue(lit[10], -2n);
assert.sameValue((function (a, b) { return a / b; })(7n, -3n), -2n);
assert.sameValue((function (a, b) { return a % b; })(7n, -3n), 1n);
assert.sameValue((function (a, b) { return a % b; })(-7n, 3n), -1n);

// No handler in the function: the RangeError leaves the chunk.
assert.throws(RangeError, function () { var a = 1n, b = 0n; return a / b; });
assert.throws(RangeError, function () { var a = 1n, b = 0n; return a % b; });
assert.throws(RangeError, function () { var a = 1n, b = -1n; return a ** b; });

function guarded(a, b) {
  var log = [];
  var sum = 100n + a;
  try {
    sum = sum + (a / b) * 2n;
  } catch (e) {
    log.push(e.constructor.name);
  } finally {
    log.push("finally");
  }
  try {
    sum = sum + (a % b);
  } catch (e) {
    log.push(e.constructor.name);
  }
  try {
    sum = sum + (a ** b);
  } catch (e) {
    log.push(e.constructor.name);
  }
  return [sum, log.join()];
}
var r = guarded(10n, 0n);
assert.sameValue(r[0], 111n, "operands of the failed operators are discarded");
assert.sameValue(r[1], "RangeError,finally,RangeError");
r = guarded(10n, -1n);
assert.sameValue(r[1], "finally,RangeError", "negative exponent after a defined remainder and quotient");
assert.sameValue(r[0], 100n + 10n + (10n / -1n) * 2n + (10n % -1n));

function nestedFailure(a, b) {
  var stack = [];
  for (var i = 0n; i < 3n; i++) {
    try {
      stack.push(i * 2n + a / b + 1n);
    } catch (e) {
      stack.push(-i);
    }
  }
  return stack;
}
var s = nestedFailure(1n, 0n);
assert.sameValue(s.length, 3);
assert.sameValue(s[0], 0n);
assert.sameValue(s[1], -1n);
assert.sameValue(s[2], -2n);

function* gen(a, b) {
  yield a + b;
  yield a / b;
}
var g = gen(6n, 0n);
assert.sameValue(g.next().value, 6n);
assert.throws(RangeError, function () { g.next(); });

function shifts(x) {
  return [x << 0n, x >> 0n, x << 64n, x >> 64n, x << -1n, x >> -1n, -x >> 1n, -x >> 1000n, x >> 1000n];
}
var sh = shifts(big);
assert.sameValue(sh[0], big);
assert.sameValue(sh[1], big);
assert.sameValue(sh[2], big * 18446744073709551616n);
assert.sameValue(sh[3], big / 18446744073709551616n);
assert.sameValue(sh[4], big >> 1n);
assert.sameValue(sh[5], big << 1n);
assert.sameValue(sh[6], -((big + 1n) / 2n));
assert.sameValue(sh[7], -1n);
assert.sameValue(sh[8], 0n);

assert.throws(TypeError, function () { var a = big; var b = 3n; return a >>> b; });
assert.throws(TypeError, function () { var a = big; return a + 1; });
assert.throws(TypeError, function () { var a = big; var b = 1; return a < b ? 1 : (a * b); });
