// Theme: alloc-gc. Short-lived object-literal allocation (nursery churn, no live set).
// Models: JetStream splay GeneratePayloadTree internal nodes ({left, right}), ai-astar neighbour records.
// Each iteration allocates one {a,b,c,d} literal that dies immediately; the loop is
// small enough that cost is dominated by object allocation + minor GC, not by calls.
// Measured (buildbox3, median of 5, load 4.5, jsse HEAD 2b621c5 vs boa 0.22.0, 2026-10-10): jsse 4.08 s / boa 2.53 s = 1.62x; jsse --no-bytecode 4.18 s. GC is ~3% of runtime: this gap is construction/call overhead.
var seed = 12345;
function lcg() {
  seed = (Math.imul(seed, 1103515245) + 12345) & 0x7fffffff;
  return seed;
}
var N = 400000;
var sum = 0;
for (var i = 0; i < N; i++) {
  var o = { a: i, b: i + 1, c: null, d: lcg() & 255 };
  sum = (sum + o.a + o.b + o.d) | 0;
}
console.log("objlit_churn", N, sum);
