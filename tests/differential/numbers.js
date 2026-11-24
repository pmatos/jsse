// Number formatting is a classic source of silent divergence.
const xs = [0, -0, 1 / 3, 0.1 + 0.2, 1e21, 1e-7, 123456789.125, 2 ** 53, -1.5e-10, NaN, -Infinity];
for (const x of xs) {
  show(String(x), x.toFixed(2), x.toPrecision(4), x.toExponential(3));
}
for (const radix of [2, 8, 16, 36]) {
  show(radix, (255).toString(radix), (-0.5).toString(radix), (1e10 + 0.25).toString(radix));
}
show(parseInt("0x1f"), parseInt("08"), parseFloat("3.14abc"), Number(""), Number(" 12 "), Number("1_000"));
show(Math.round(-0.5), Math.round(2.5), Math.max(), Math.min(), Math.sign(-0), Object.is(Math.sign(-0), -0));
show(Number.MAX_SAFE_INTEGER, Number.EPSILON, Number.MIN_VALUE, (25).toString(36));
