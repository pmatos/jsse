const big = 2n ** 100n;
show(big, big.toString(16), -big / 3n, -7n % 3n, 7n >> 1n, -7n >> 1n, ~5n);
show(BigInt.asIntN(8, 255n), BigInt.asUintN(8, -1n), BigInt("0x1f"), typeof 1n, 1n == 1, 1n === 1, 2n > 1);
show(String(10n ** 30n), (123n).toLocaleString === undefined, [3n, 1n, 2n].sort().join());
try { 1n + 1; } catch (e) { show(e.name); }
show(Number(2n ** 64n), parseInt("9007199254740993"), BigInt(Number.MAX_SAFE_INTEGER) + 2n);
