// bench_bigint_mulmod256.js
// Exercises: BigInt multiply + remainder on secp256k1-field-sized (256-bit) operands,
//   via noble's `mod(a, b = P)` helper (a % b, then `result >= 0n ? result : b + result`).
//   One iteration = 2 mul + 2 mod + 1 add + 2 compares: pure op-dispatch + alloc cost.
// Models: bigint-noble-secp256k1, bigint-noble-ed25519, bigint-noble-bls12-381 (field arith).
// Measured 2026-10-10, jsse 2b621c5 (v0.11.0) vs boa 0.22.0 on buildbox3 (EPYC 7501), median of 5:
//   jsse 2.67 s / boa 0.79 s = 3.40x slower; jsse --no-bytecode 2.68 s; loadavg1 <= 7.3.
const P = 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffc2fn;
const _0n = 0n;
function mod(a, b = P) {
  const result = a % b;
  return result >= _0n ? result : b + result;
}
let x = 0x79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798n;
let y = 0x483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8n;
let acc = 0n;
const N = 150000;
for (let i = 0; i < N; i++) {
  x = mod(x * y);
  y = mod(y * y + x);
  acc = mod(acc + x);
}
console.log("mulmod256", (acc % 1000000007n).toString(), (x % 1000003n).toString());
