// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3 at loadavg ~7, median of 5: jsse 4.53 s, boa 0.22 s, ratio 21.07x; jsse --no-bytecode 4.53 s (median of 3).
// Exercises: for-of / spread / Array.from / forEach over Map and Set with a few
// thousand entries (entries()/keys()/values() iterators and [Symbol.iterator]).
// Models: sync-fs `for (let [name, entry] of this.structure)` (Directory.ls()),
// lazy-collections `unique()`/Set consumers.
// Isolates the cost of one Map/Set iterator next() step as a function of size.
const N = 8000;
const m = new Map();
for (let i = 0; i < N; i++) m.set(i, i * 2);
const s = new Set();
for (let i = 0; i < N; i++) s.add(i);
let acc = 0;
for (let r = 0; r < 2; r++) {
  for (const [k, v] of m) acc = (acc + k + v) | 0;
  for (const k of m.keys()) acc = (acc + k) | 0;
  for (const v of s) acc = (acc + v) | 0;
  acc = (acc + [...m].length + [...s].length + Array.from(m.values()).length) | 0;
}
console.log("map_set_iteration", acc);
