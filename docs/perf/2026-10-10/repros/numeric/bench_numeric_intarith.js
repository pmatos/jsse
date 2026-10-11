// Theme: numeric. Pure int32 register arithmetic on function-local `var`s: no
// arrays, no calls, no property access. One SHA-256-style compression round
// body (rotates, xor, and/not, 32-bit adds) iterated in a hot loop.
// Models: stanford-crypto-sha256 / pbkdf2 / aes, crypto, mandreel register traffic
// (the "arith" phase of benchmarks/scripts/bench_opmix.js).
// Isolates: VM dispatch + local-variable access + int32 bit-op conversion cost.
// Measured (buildbox3, 2026-10-10, jsse HEAD 2b621c5 / boa 0.22.0, median of 5): jsse 3.23 s, boa 0.97 s, ratio 3.33x, jsse --no-bytecode 5.69 s.
function rounds(n) {
  var a = 0x6a09e667 | 0, b = 0xbb67ae85 | 0, c = 0x3c6ef372 | 0, d = 0xa54ff53a | 0;
  var e = 0x510e527f | 0, f = 0x9b05688c | 0, g = 0x1f83d9ab | 0, h = 0x5be0cd19 | 0;
  var k = 0x428a2f98 | 0;
  for (var i = 0; i < n; i++) {
    var s1 = ((e >>> 6) | (e << 26)) ^ ((e >>> 11) | (e << 21)) ^ ((e >>> 25) | (e << 7));
    var ch = (e & f) ^ (~e & g);
    var t1 = (h + s1 + ch + k + i) | 0;
    var s0 = ((a >>> 2) | (a << 30)) ^ ((a >>> 13) | (a << 19)) ^ ((a >>> 22) | (a << 10));
    var maj = (a & b) ^ (a & c) ^ (b & c);
    var t2 = (s0 + maj) | 0;
    h = g; g = f; f = e;
    e = (d + t1) | 0;
    d = c; c = b; b = a;
    a = (t1 + t2) | 0;
  }
  return (a ^ b ^ c ^ d ^ e ^ f ^ g ^ h) >>> 0;
}
console.log("intarith", rounds(500000));
