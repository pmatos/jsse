// Theme: alloc-gc. High-survival allocation: fill a table with 250k retained {key, a, b} records
// (every allocation stays live), then overwrite random slots 350k times so a steady stream of new
// objects survives while old ones die (the splay.js steady state: ~95% of each nursery survives).
// A generational collector should cost O(nursery) per collection here; a pacer that answers
// "high survival" with a full collection every few nurseries costs O(live set) each time,
// so total time grows quadratically with the retained size.
// Models: JetStream splay (setup + run), ai-astar Graph construction, any "build a big structure" phase.
// Measured (buildbox3, median of 5, load 4.5, jsse HEAD 2b621c5 vs boa 0.22.0, 2026-10-10): jsse 6.54 s / boa 3.03 s = 2.16x; jsse --no-bytecode 6.54 s. Dropping the high-survival minor suppression (WI-1 experiment) takes jsse to ~5.5 s (-14%); the rest is mutator cost plus whole-array re-scans in minors.
var seed = 20240607;
var N = 250000;
var table = new Array(N);
for (var i = 0; i < N; i++) table[i] = { key: i, a: i, b: null };
for (var r = 0; r < 350000; r++) {
  seed = (Math.imul(seed, 1103515245) + 12345) & 0x7fffffff;
  table[seed % N] = { key: r, a: r, b: null };
}
var s = 0;
for (var i = 0; i < N; i++) s = (s + table[i].key + table[i].a) | 0;
console.log("retain_grow", N, s);
