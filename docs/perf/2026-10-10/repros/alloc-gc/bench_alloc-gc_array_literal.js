// Theme: alloc-gc. Short-lived small array-literal allocation: [0,1,...,9] and [a, b].
// Models: JetStream splay leaf payload (`array: [0..9]`), Octane/boyer cons lists, ai-astar neighbour arrays.
// Nothing is retained. Isolates array creation (element storage + index property bookkeeping) and its GC.
// Measured (buildbox3, median of 5, load 3.2, jsse HEAD 2b621c5 vs boa 0.22.0, 2026-10-10): jsse 2.17 s / boa 0.82 s = 2.66x; jsse --no-bytecode 2.12 s.
var N = 150000;
var sum = 0;
for (var i = 0; i < N; i++) {
  var a = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
  var b = [i, a];
  sum = (sum + a[9] + b[0] + b[1].length) | 0;
}
console.log("array_literal", N, sum);
