// bench_bigint_ops_vm_add.js
// Exercises: a single BigInt `+` / `*` / `&` / `>>` per iteration on 256-bit operands inside a
//   VM-compiled loop (no calls). Use together with bench_bigint_ops_vm_numadd.js (the same
//   statements on Numbers) to separate "BigInt-specific cost per op" from "VM per-statement cost".
// Models: bigint-noble-* inner arithmetic.
// Measured 2026-10-10, jsse 2b621c5 (v0.11.0) vs boa 0.22.0 on buildbox3 (EPYC 7501), median of 5:
//   jsse 2.83 s / boa 0.97 s = 2.92x slower; jsse --no-bytecode 5.14 s; loadavg1 <= 7.6.
function main() {
  var a = BigInt("0x79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798");
  var b = BigInt("0x483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8");
  var s = BigInt(2);
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
console.log("ops_vm_add", main());
