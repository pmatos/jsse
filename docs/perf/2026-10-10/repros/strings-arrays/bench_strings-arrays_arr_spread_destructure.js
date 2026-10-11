// `var [a, b] = q` and `[...q, a]` on a 3-element array, 300k iterations.
// Models: modern-JS code (Babylon, ML, FlightPlanner).
// Cause: destructuring/spread go through the generic iterator protocol (iterator object, next() result objects) with no Array fast path.
// Measured (jsse v0.11.0 @ 2b621c5 vs boa 0.22.0, median of 5, buildbox3, 2026-10-10): global-scope jsse 6.59 s / boa 3.48 s = 1.90x; function-scope variant (fn-scope/) jsse 6.05 s / boa 2.37 s = 2.55x.
// Prints one checksum line; jsse and boa print the same line. Deterministic (LCG, no Date/Math.random).
// Global-scope variant (top-level `var`): isolates the builtin cost; the scalar-loop control (ctl_scalar_global) defines the baseline ratio here.
var acc = 0;
var q = [1, 2, 3];
for (var i = 0; i < 300000; i++) {
  var [a, b] = q;
  var r = [...q, a];
  acc += r.length + b;
}
console.log(acc);
