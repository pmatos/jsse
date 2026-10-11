// bench_bigint_literal.js
// Exercises: BigInt literal evaluation in a hot loop (`x + 10n`, `x % 2n === 0n`,
//   `x * 1000003n`). jsse re-parses the literal text on every evaluation, and the bytecode
//   compiler bails on BigInt literals (BAIL "literal"), forcing the whole function onto
//   the tree-walker.
// Models: bigint-bigdenary (`0n`, `10n`, `1n`, `-1n` in countTrailingZeros/scaleDecimalsTo),
//   bigint-paillier (`2n`, `0n`, `1n` in modPow / isProbablyPrime).
// Measured 2026-10-10, jsse 2b621c5 (v0.11.0) vs boa 0.22.0 on buildbox3 (EPYC 7501), median of 5:
//   jsse 2.77 s / boa 0.53 s = 5.26x slower; jsse --no-bytecode 2.46 s; loadavg1 <= 7.5.
function main() {
  var x = BigInt(123456789);
  var k = 0;
  for (var i = 0; i < 400000; i++) {
    var r = x + 10n;
    if (r % 2n === 0n) k += 1;
    var q = x * 1000003n - 1n;
    if (q > 0n) k += 1;
    x = (x + 7n) % 4294967311n;
  }
  return k + Number(x);
}
console.log("literal", main());
