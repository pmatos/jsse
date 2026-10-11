// Theme: alloc-gc. CONTROL for the hash-map investigation: built-in Map/Set insert, lookup, delete.
// Same shape as JetStream hash-map (90000 integer keys, then repeated gets) but through the
// native collections, so the JS-level object/array/call cost of hand-written hash tables is absent.
// Measured (buildbox3, median of 5, load 4.4, jsse HEAD 2b621c5 vs boa 0.22.0, 2026-10-10): jsse 3.48 s / boa 0.52 s = 6.72x; jsse --no-bytecode 3.48 s.
var COUNT = 8000;
var m = new Map();
for (var i = 0; i < COUNT; i++) m.set(i, 42);
var result = 0;
for (var j = 0; j < 5; j++)
  for (var i = 0; i < COUNT; i++) result += m.get(i);
var s = new Set();
for (var i = 0; i < COUNT; i++) s.add(i * 7);
var hits = 0;
for (var j = 0; j < 3; j++)
  for (var i = 0; i < COUNT * 7; i += 7) if (s.has(i)) hits++;
for (var i = 0; i < COUNT; i += 2) { m.delete(i); s.delete(i * 7); }
console.log("native_map", result, hits, m.size, s.size);
