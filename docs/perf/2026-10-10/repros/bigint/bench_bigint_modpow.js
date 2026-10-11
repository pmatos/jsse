// bench_bigint_modpow.js
// Exercises: square-and-multiply modPow, written the way paillier-bigint / bigint-mod-arith
//   write it: `e % 2n === 1n`, `e /= 2n`, `n ** 2n % r` on 512..1024-bit moduli.
//   Mixes real multiprecision work (1024-bit mul/rem) with the small-operand control ops.
// Large-operand case: 1024-bit multiply+remainder dominate, so num-bigint (same crate in both
//   engines) does most of the work; the remaining gap is the fixed per-operator overhead. Models: bigint-paillier (encrypt/decrypt = modPow over n^2, 2048-bit).
// Measured 2026-10-10, jsse 2b621c5 (v0.11.0) vs boa 0.22.0 on buildbox3 (EPYC 7501), median of 5:
//   jsse 2.26 s / boa 1.14 s = 1.98x slower; jsse --no-bytecode 2.57 s; loadavg1 <= 7.7.
// Role: LARGE-OPERAND repro (arithmetic-dominated; smallest gap in the set).
function modPow(b, e, n) {
  let r = 1n;
  b = b % n;
  while (e > 0n) {
    if (e % 2n === 1n) r = (r * b) % n;
    e /= 2n;
    b = b ** 2n % n;
  }
  return r;
}
let m = (1n << 1024n) - 105n;
let x = 0x123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef1n;
let e = (1n << 400n) + 0xfffffffffffffffn;
let acc = 0n;
for (let i = 0; i < 1000; i++) {
  x = modPow(x, e, m) + 3n;
  acc = (acc + x) % m;
}
console.log("modpow", (acc % 1000000007n).toString());
