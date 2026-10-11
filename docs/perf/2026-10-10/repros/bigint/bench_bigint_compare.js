// bench_bigint_compare.js
// Exercises: BigInt equality / relational comparisons only (===, !==, <, >=, <=),
//   256-bit vs 256-bit, vs 0n, and BigInt vs Number. No arithmetic in the loop.
// Models: noble `if (x === _0n)`, `a >= _0n`, `modulo <= _0n`, bigdenary `base < 0`.
// Measured 2026-10-10, jsse 2b621c5 (v0.11.0) vs boa 0.22.0 on buildbox3 (EPYC 7501), median of 5:
//   jsse 6.40 s / boa 1.29 s = 4.95x slower; jsse --no-bytecode 6.40 s; loadavg1 <= 8.8.
// Role: GENERIC-COST repro (see bench_bigint_compare_numeric_control.js: Numbers cost the same).
const vals = [];
let s = 0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdefn;
for (let i = 0; i < 64; i++) {
  vals.push(i % 5 === 0 ? 0n : (i % 2 ? s : -s));
  s = (s * 6364136223846793005n + 1442695040888963407n) & ((1n << 256n) - 1n);
}
const zero = 0n;
let c = 0;
for (let r = 0; r < 12000; r++) {
  for (let i = 0; i < 64; i++) {
    const a = vals[i];
    const b = vals[(i + r) & 63];
    if (a === zero) c++;
    if (a !== b) c += 2;
    if (a < b) c += 3;
    if (a >= zero) c += 5;
    if (b <= a) c += 7;
    if (a < 0) c += 11;
    if (a > 1) c += 13;
  }
}
console.log("compare", c);
