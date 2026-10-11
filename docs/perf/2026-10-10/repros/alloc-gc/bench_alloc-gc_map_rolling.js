// Theme: alloc-gc. Built-in Map/Set used as a bounded cache / sliding window: insert key i, delete
// key i-W, look up recent keys. The live size stays at W (=2000) entries, but the number of
// insert+delete operations is large. Exercises Map/Set get/set/has/delete on a modest live set
// plus whatever the engine does with deleted slots (tombstones in an insertion-ordered store).
// Models: LRU caches, visited-sets in graph search (ai-astar style), de-dup windows.
// Measured (buildbox3, median of 5, load 4.6, jsse HEAD 2b621c5 vs boa 0.22.0, 2026-10-10): jsse 3.43 s / boa 0.57 s = 6.05x; jsse --no-bytecode 3.48 s.
var W = 2000;
var OPS = 20000;
var m = new Map();
var s = new Set();
var hits = 0;
for (var i = 0; i < OPS; i++) {
  m.set(i, i * 2);
  s.add(i);
  if (i >= W) {
    m.delete(i - W);
    s.delete(i - W);
  }
  if (m.has(i - 5)) hits++;
  if (s.has(i - 7)) hits++;
  if (m.get(i - 11) === (i - 11) * 2) hits++;
}
console.log("map_rolling", m.size, s.size, hits);
