// Theme: alloc-gc. Short-lived constructor allocation: `new Node(k, v)` with this.x = ... stores.
// Models: JetStream splay SplayTree.Node, hash-map Entry, early-boyer sc_Pair (cons cells).
// Nothing is retained, so this isolates `new` + 2-4 property stores + GC of dead objects.
// Measured (buildbox3, median of 5, load 4.1, jsse HEAD 2b621c5 vs boa 0.22.0, 2026-10-10): jsse 4.18 s / boa 2.57 s = 1.62x; jsse --no-bytecode 4.53 s.
function Node(key, value) {
  this.key = key;
  this.value = value;
}
Node.prototype.left = null;
Node.prototype.right = null;
function Pair(car, cdr) {
  this.car = car;
  this.cdr = cdr;
}
var N = 300000;
var sum = 0;
for (var i = 0; i < N; i++) {
  var n = new Node(i, i + 1);
  var p = new Pair(n, null);
  sum = (sum + p.car.key + p.car.value) | 0;
}
console.log("ctor_churn", N, sum);
