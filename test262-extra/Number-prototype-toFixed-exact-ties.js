/*---
description: >
  Number.prototype.toFixed rounds an exact tie up (to the larger n), not to
  even, and otherwise rounds the exact binary value of the number.
esid: sec-number.prototype.tofixed
info: |
  Number.prototype.toFixed ( fractionDigits ), step 10.a: Let n be an integer
  for which n / 10^f - x is as close to zero as possible. If there are two
  such n, pick the larger n.
---*/

assert.sameValue((0.5).toFixed(0), "1", "0.5 is an exact tie");
assert.sameValue((1.5).toFixed(0), "2", "1.5 is an exact tie");
assert.sameValue((2.5).toFixed(0), "3", "2.5 is an exact tie");
assert.sameValue((-2.5).toFixed(0), "-3", "the sign is applied after rounding the magnitude");
assert.sameValue((0.125).toFixed(2), "0.13", "0.125 is an exact tie");
assert.sameValue((123456789.125).toFixed(2), "123456789.13", "large exact tie");
assert.sameValue((9.5).toFixed(0), "10", "a tie that carries into a new digit");
assert.sameValue((1.005).toFixed(2), "1.00", "1.005 is slightly below the tie in binary");
assert.sameValue((8.345).toFixed(2), "8.35", "8.345 is slightly above the tie in binary");
assert.sameValue((1.45).toFixed(1), "1.4", "1.45 is slightly below the tie in binary");
assert.sameValue((-0.5).toFixed(0), "-1", "negative exact tie");
assert.sameValue((-0).toFixed(2), "0.00", "-0 formats as 0");
