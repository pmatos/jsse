// bench_bigint_ops_vm_numadd.js
// CONTROL for bench_bigint_ops_vm_add.js: identical loop, Number operands.
// Measured 2026-10-10, jsse 2b621c5 (v0.11.0) vs boa 0.22.0 on buildbox3 (EPYC 7501), median of 5:
//   jsse 1.92 s / boa 0.52 s = 3.72x slower; jsse --no-bytecode 3.98 s; loadavg1 <= 6.5.
// Role: SHAPE CONTROL (Number operands; the delta to bench_bigint_ops_vm_add is the BigInt-specific cost).
function main() {
  var a = 123456789, b = 987654321, s = 2;
  var r = a, k = 0;
  for (var i = 0; i < 1200000; i++) {
    r = a + b;
    r = r * s;
    r = r & a;
    r = r >> s;
    k += 1;
  }
  return k + (r > a ? 1 : 0);
}
console.log("ops_vm_numadd", main());
