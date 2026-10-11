// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3 at loadavg ~7, median of 5: jsse 4.99 s, boa 1.72 s, ratio 2.89x; jsse --no-bytecode 4.98 s (median of 3). CONTROL (no generators).
// CONTROL: for-of / spread over plain arrays with no generator involvement.
// Shows how the per-element cost of the iterator protocol on a built-in
// iterator (ArrayIterator.next -> iter-result object -> done/value Gets)
// compares with boa. Map/Set are covered by bench_generators_map_set_iteration.js.
const arr = [];
for (let i = 0; i < 100000; i++) arr.push(i);
let sum = 0;
for (let r = 0; r < 12; r++) {
  for (const v of arr) sum = (sum + v) | 0;
}
let copy = 0;
for (let r = 0; r < 6; r++) copy += [...arr].length;
console.log("array_forof_control", sum, copy);
