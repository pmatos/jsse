// bench_bigint_compare_numeric_control.js
// CONTROL for bench_bigint_compare.js: identical program with Number operands instead of
//   BigInt. Shows that the BigInt comparison loop costs the same as the Number one in jsse,
//   i.e. the gap in bench_bigint_compare.js is the generic tree-walker/lexical-declaration
//   cost, not BigInt comparison.
// Measured 2026-10-10, jsse 2b621c5 (v0.11.0) vs boa 0.22.0 on buildbox3 (EPYC 7501), median of 5:
//   jsse 6.03 s / boa 0.99 s = 6.12x slower; jsse --no-bytecode 6.65 s; loadavg1 <= 7.6.
// Role: SHAPE CONTROL (BigInt-free; same cost as the BigInt twin = BigInt compare is not the cause).
const vals = [];
let s = 123456789;
for (let i = 0; i < 64; i++) {
  vals.push(i % 5 === 0 ? 0 : (i % 2 ? s : -s));
  s = (s * 1103515245 + 12345) & 0x7fffffff;
}
const zero = 0;
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
console.log("compare_num", c);
