// ToInt32 (§7.1.6) and ToUint32 (§7.1.7) must reduce the truncated real value
// modulo 2^32 across the whole Number range. For magnitudes >= 2^63 a naive
// `truncate as i64` conversion saturates and yields the wrong 32-bit result;
// the modular reduction must be exact. Expected values cross-checked with Node.
// Spec: ECMAScript, sec-toint32, sec-touint32.

function assertEq(actual, expected, msg) {
  // Distinguish +0 from -0 as well as ordinary inequality.
  if (actual !== expected || 1 / actual !== 1 / expected) {
    throw new Test262Error(
      msg + ": expected " + expected + " but got " + actual
    );
  }
}

// Bitwise operators funnel operands through ToInt32.
assertEq((2 ** 31) | 0, -2147483648, "(2**31)|0");
assertEq((2 ** 32) | 0, 0, "(2**32)|0");
assertEq((2 ** 32 + 5) | 0, 5, "(2**32+5)|0");
assertEq((2 ** 53) | 0, 0, "(2**53)|0");
assertEq((2 ** 63) | 0, 0, "(2**63)|0");
assertEq((2 ** 64) | 0, 0, "(2**64)|0");
assertEq((1e21) | 0, -559939584, "(1e21)|0");
assertEq(~(2 ** 64), -1, "~(2**64)");
assertEq((2 ** 32 + 5) & 0xffffffff, 5, "(2**32+5) & 0xffffffff");

// Unsigned right shift funnels operands through ToUint32.
assertEq((2 ** 32) >>> 0, 0, "(2**32)>>>0");
assertEq((2 ** 64) >>> 0, 0, "(2**64)>>>0");
assertEq((1e21) >>> 0, 3735027712, "(1e21)>>>0");
assertEq(-1 >>> 0, 4294967295, "(-1)>>>0");

// Math.imul / Math.clz32 also use ToInt32 / ToUint32.
assertEq(Math.imul(2 ** 32 + 3, 2), 6, "Math.imul(2**32+3, 2)");
assertEq(Math.clz32(2 ** 32 + 1), 31, "Math.clz32(2**32+1)");

// Integer TypedArray element writes coerce via ToInt32 / ToUint32.
var i32 = new Int32Array(1);
i32[0] = 2 ** 64;
assertEq(i32[0], 0, "Int32Array <- 2**64");
i32[0] = 2 ** 32 + 5;
assertEq(i32[0], 5, "Int32Array <- 2**32+5");

var u32 = new Uint32Array(1);
u32[0] = 2 ** 64;
assertEq(u32[0], 0, "Uint32Array <- 2**64");
u32[0] = -1;
assertEq(u32[0], 4294967295, "Uint32Array <- -1");
