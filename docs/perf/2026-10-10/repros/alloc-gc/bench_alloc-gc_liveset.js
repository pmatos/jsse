// Theme: alloc-gc. GC cost versus a large, long-lived heap: build a retained tree of ~260k
// objects, then allocate 600k short-lived objects while the tree stays live.
// A generational collector pays only for the churn; a collector that re-traces the live set
// on every (or every few) collections pays proportionally to live-set size.
// Models: JetStream splay (8000-node tree, ~95 objects per node => ~760k live objects).
// Measured (buildbox3, median of 5, load 5.3, jsse HEAD 2b621c5 vs boa 0.22.0, 2026-10-10): jsse 5.84 s / boa 3.33 s = 1.75x; jsse --no-bytecode 6.34 s. GC share is only ~10% here (low survival): kept as the WI-1 must-not-regress control.
function build(depth) {
  if (depth === 0) return { v: 1, l: null, r: null };
  return { v: depth, l: build(depth - 1), r: build(depth - 1) };
}
function count(t) {
  return t === null ? 0 : 1 + count(t.l) + count(t.r);
}
var live = build(17);
var liveCount = count(live);
var sum = 0;
for (var i = 0; i < 600000; i++) {
  var o = { a: i, b: null };
  sum = (sum + o.a) | 0;
}
console.log("liveset", liveCount, sum, count(live));
