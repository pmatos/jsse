// Allocation of many tiny arrays: `[i, i + 1]` return values and `[a, b, c]` literals, 500k iterations.
// Models: pdfjs, ML, Babylon, FlightPlanner (tuples, small vectors).
// Cause: Array literals are not compiled by the bytecode compiler (BAIL expression:Array) so the enclosing body runs on the tree-walker; each array also pays object-arena + length-property + shape_id setup.
// Measured (jsse v0.11.0 @ 2b621c5 vs boa 0.22.0, median of 5, buildbox3, 2026-10-10): global-scope jsse 5.79 s / boa 2.58 s = 2.25x; function-scope variant (fn-scope/) jsse 5.48 s / boa 1.04 s = 5.26x.
// Prints one checksum line; jsse and boa print the same line. Deterministic (LCG, no Date/Math.random).
// Function-scope variant: realistic (JetStream code runs inside functions) but includes jsse's ~4x local-variable overhead (see ctl_scalar_fn), which belongs to another theme.
(function () {
function pair(i) { return [i, i + 1]; }
var acc = 0;
for (var i = 0; i < 500000; i++) {
  var p = pair(i);
  var q = [p[0], p[1], p[0] + p[1]];
  acc += q.length + q[2];
}
console.log(acc);
})();
