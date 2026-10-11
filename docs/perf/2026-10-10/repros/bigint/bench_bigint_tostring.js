// bench_bigint_tostring.js
// Exercises: BigInt.prototype.toString(16) and toString() on 256-bit values from a hot loop
//   (primitive-receiver method call + radix conversion + result string creation). No other
//   BigInt work. Written with `var`-only code in one function so the bytecode VM compiles it.
// Models: noble hex helpers (bytesToHex / numTo32bStr / toString(16).padStart) and
//   bigdenary's `base.toString()`.
// Measured 2026-10-10, jsse 2b621c5 (v0.11.0) vs boa 0.22.0 on buildbox3 (EPYC 7501), median of 5:
//   jsse 1.27 s / boa 0.37 s = 3.47x slower; jsse --no-bytecode 1.12 s; loadavg1 <= 4.2.
function main() {
  var a = BigInt("0x79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798");
  var b = BigInt("0x483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8");
  var total = 0;
  for (var i = 0; i < 150000; i++) {
    var h = a.toString(16);
    var d = b.toString();
    total += h.length + d.length + h.charCodeAt(i & 31);
  }
  return total;
}
console.log("tostring", main());
