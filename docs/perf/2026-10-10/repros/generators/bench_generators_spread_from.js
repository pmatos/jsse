// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3 at loadavg ~7, median of 5: jsse 2.12 s, boa 0.52 s, ratio 4.11x; jsse --no-bytecode 2.12 s (median of 3).
// Exercises: consuming generators through spread, Array.from, destructuring
// and `yield*` of a plain array, instead of for-of: the non-for-of
// iteration-protocol consumers.
// Models: lazy-collections (toArray/flatten/windows), js-tokens consumers.
function* gen(n) { for (let i = 0; i < n; i++) yield i; }
function* twice(a) { yield* a; yield* a; }
let acc = 0;
for (let r = 0; r < 60; r++) {
  const a = [...gen(2000)];
  const b = Array.from(gen(2000));
  const c = Array.from(twice(a.slice(0, 500)));
  const [x, y, ...rest] = gen(1500);
  acc = (acc + a.length + b[1999] + c.length + x + y + rest.length) | 0;
  const s = new Set(gen(500));
  acc = (acc + s.size) | 0;
}
console.log("spread_from", acc);
