// bench_bigint_mulmod256_vm.js
// Exercises: the same 256-bit multiply + remainder loop as bench_bigint_mulmod256.js, but
//   written with only constructs the bytecode compiler accepts (var, no BigInt literals, no
//   default params, no destructuring), so the whole workload runs on the VM. Isolates VM
//   per-op / call / name-resolution cost + BigInt op cost without the tree-walker fallback.
// Models: bigint-noble-* (field arithmetic hot loop).
// Measured 2026-10-10, jsse 2b621c5 (v0.11.0) vs boa 0.22.0 on buildbox3 (EPYC 7501), median of 5:
//   jsse 1.96 s / boa 0.68 s = 2.86x slower; jsse --no-bytecode 2.02 s; loadavg1 <= 6.6.
var P = BigInt("0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffc2f");
var Z = BigInt(0);
function mod(a, b) {
  var result = a % b;
  return result >= Z ? result : b + result;
}
function main() {
  var x = BigInt("0x79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798");
  var y = BigInt("0x483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8");
  var acc = BigInt(0);
  for (var i = 0; i < 150000; i++) {
    x = mod(x * y, P);
    y = mod(y * y + x, P);
    acc = mod(acc + x, P);
  }
  return acc;
}
var acc = main();
console.log("mulmod256", (acc % BigInt(1000000007)).toString());
