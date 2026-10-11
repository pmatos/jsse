// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3 at loadavg ~7, median of 5: jsse 6.34 s, boa 1.27 s, ratio 5.00x; jsse --no-bytecode 6.39 s (median of 3).
// Exercises: a lazy-collections style pipeline: range -> map -> filter ->
// takeWhile -> reduce, every stage a generator that does `for (const x of
// upstream) yield ...`. Each element crosses 4 generator boundaries.
// Models: lazy-collections (filter/map/takeWhile/toArray over range()).
function* range(lo, hi) {
  for (let i = lo; i <= hi; i++) yield i;
}
function map(fn) {
  return function (data) {
    return { *[Symbol.iterator]() { let i = 0; for (const d of data) yield fn(d, i++); } };
  };
}
function filter(fn) {
  return function (data) {
    return { *[Symbol.iterator]() {
      let i = 0;
      for (const d of data) { if (!fn(d, i++)) continue; yield d; }
    } };
  };
}
function takeWhile(fn) {
  return function (data) {
    return { *[Symbol.iterator]() {
      let i = 0;
      for (const d of data) { if (!fn(d, i++)) return; yield d; }
    } };
  };
}
function reduce(fn, init) {
  return function (data) { let acc = init, i = 0; for (const d of data) acc = fn(acc, d, i++); return acc; };
}
const N = 120000;
let total = 0;
for (let r = 0; r < 2; r++) {
  const p = reduce((a, x) => (a + x) | 0, 0)(
    takeWhile(x => x < 1e9)(
      filter(x => x % 3 !== 0)(
        map(x => x * 2)(range(1, N)))));
  total = (total + p) | 0;
}
console.log("pipeline", total);
