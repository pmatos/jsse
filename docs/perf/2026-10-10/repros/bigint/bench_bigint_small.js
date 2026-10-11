// bench_bigint_small.js
// Exercises: BigInt arithmetic on tiny (< 64-bit) operands, including BigInt-vs-Number
//   relational compares: `% 10n === 0n`, `/ 10n`, `+= 1n`, `n < 0`, `e % 2n === 1n`, `e /= 2n`.
//   Isolates per-op fixed overhead (dispatch/ToNumeric/Arc alloc) with no real arithmetic cost.
// Models: bigint-bigdenary (countTrailingZeros, bigIntAbs), bigint-paillier (modPow loop).
// Measured 2026-10-10, jsse 2b621c5 (v0.11.0) vs boa 0.22.0 on buildbox3 (EPYC 7501), median of 5:
//   jsse 2.17 s / boa 0.47 s = 4.66x slower; jsse --no-bytecode 2.17 s; loadavg1 <= 5.0.
function countTrailingZeros(n, upTo) {
  if (n === 0n) return 0;
  let count = 0;
  let c = n < 0 ? n * -1n : n;
  while (c % 10n === 0n && count < upTo) {
    count += 1;
    c = c / 10n;
  }
  return count;
}
function collatzSteps(n) {
  let steps = 0;
  while (n !== 1n) {
    if (n % 2n === 1n) n = 3n * n + 1n;
    else n /= 2n;
    steps++;
  }
  return steps;
}
let total = 0;
let k = 1n;
for (let i = 0; i < 60000; i++) {
  const v = k * 1000n;
  total += countTrailingZeros(i % 7 === 0 ? -v : v, 20);
  k += 7n;
}
for (let i = 1; i <= 2500; i++) total += collatzSteps(BigInt(i) * 7919n + 1n);
console.log("small", total);
