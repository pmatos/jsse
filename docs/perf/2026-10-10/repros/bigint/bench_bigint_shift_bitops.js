// bench_bigint_shift_bitops.js
// Exercises: BigInt <<, >>, &, | , ^ and `>>=` on 256-bit operands, in the idioms noble uses:
//   bytes -> number via `(n << 8n) | BigInt(byte)`, double-and-add scalar loops with
//   `k & 1n`, `k >>= 1n`, and bit tests.
// Models: bigint-noble-secp256k1 (precompute/multiply windows), bigint-noble-ed25519,
//   bigint-paillier (bytesToBigint).
// Measured 2026-10-10, jsse 2b621c5 (v0.11.0) vs boa 0.22.0 on buildbox3 (EPYC 7501), median of 5:
//   jsse 1.12 s / boa 0.32 s = 3.54x slower; jsse --no-bytecode 1.12 s; loadavg1 <= 5.4.
let seed = 12345;
function lcg() { seed = (seed * 1103515245 + 12345) & 0x7fffffff; return seed; }
const bytes = new Uint8Array(32);
function bytesToNumber(b) {
  let n = 0n;
  for (let i = 0; i < b.length; i++) n = (n << 8n) | BigInt(b[i]);
  return n;
}
let acc = 0n;
let ones = 0;
for (let r = 0; r < 1500; r++) {
  for (let i = 0; i < 32; i++) bytes[i] = lcg() & 0xff;
  let k = bytesToNumber(bytes);
  acc ^= k;
  while (k > 0n) {
    if (k & 1n) ones++;
    k >>= 1n;
  }
  acc = (acc << 3n | acc >> 253n) & ((1n << 256n) - 1n);
}
console.log("shift_bitops", ones, (acc % 1000000007n).toString());
