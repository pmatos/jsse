// bench_bigint_invert.js
// Exercises: extended-Euclid modular inverse (BigInt div, rem, mul, sub on shrinking
//   operands, destructuring-free hot loop) — noble's `invert()`; ~150 iterations of
//   5 BigInt ops per call on 256-bit numbers.
// Models: bigint-noble-secp256k1, bigint-noble-ed25519, bigint-noble-bls12-381 (invert), paillier.
// Measured 2026-10-10, jsse 2b621c5 (v0.11.0) vs boa 0.22.0 on buildbox3 (EPYC 7501), median of 5:
//   jsse 1.85 s / boa 0.37 s = 4.95x slower; jsse --no-bytecode 1.86 s; loadavg1 <= 7.2.
const P = 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffc2fn;
function mod(a, b = P) {
  const result = a % b;
  return result >= 0n ? result : b + result;
}
function invert(number, modulo = P) {
  let a = mod(number, modulo);
  let b = modulo;
  let x = 0n, y = 1n, u = 1n, v = 0n;
  while (a !== 0n) {
    const q = b / a;
    const r = b % a;
    const m = x - u * q;
    const n = y - v * q;
    b = a, a = r, x = u, y = v, u = m, v = n;
  }
  if (b !== 1n) throw new Error("no inverse");
  return mod(x, modulo);
}
let v = 0x79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798n;
let acc = 0n;
for (let i = 0; i < 1500; i++) {
  const inv = invert(v);
  acc = mod(acc + inv);
  v = mod(v * 0x100000001b3n + inv + 1n);
}
console.log("invert", (acc % 1000000007n).toString(), (v % 1000003n).toString());
