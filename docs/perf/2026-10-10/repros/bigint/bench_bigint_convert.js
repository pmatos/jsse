// bench_bigint_convert.js
// Exercises: BigInt <-> string / Number / fixed-width conversions: BigInt('0x'+hex),
//   toString(16), toString(10), padStart, BigInt(number), Number(bigint), BigInt.asUintN.
// Models: noble hex<->number (numTo32bStr, hexToNumber), bigdenary (n.toString(), BigInt(str)).
// Measured 2026-10-10, jsse 2b621c5 (v0.11.0) vs boa 0.22.0 on buildbox3 (EPYC 7501), median of 5:
//   jsse 2.01 s / boa 0.84 s = 2.40x slower; jsse --no-bytecode 1.96 s; loadavg1 <= 6.6.
const M256 = (1n << 256n) - 1n;
let s = 0xdeadbeefcafebabe0123456789abcdeffedcba9876543210a5a5a5a5c3c3c3c3n;
let h = 0;
let total = 0;
for (let i = 0; i < 60000; i++) {
  const hex = s.toString(16).padStart(64, "0");
  const back = BigInt("0x" + hex);
  const dec = back.toString();
  const again = BigInt(dec);
  total += hex.length + dec.length;
  h = (h * 31 + hex.charCodeAt(i & 63)) | 0;
  const w = BigInt.asUintN(64, again * 3n);
  total += Number(w & 0xffn);
  total += Number(BigInt.asUintN(32, again)) & 0xff;
  total += Number(BigInt(i) * 12345n);
  s = BigInt.asUintN(256, again * 6364136223846793005n + 1442695040888963407n);
}
console.log("convert", total, h, (s & 0xffffffffn).toString());
