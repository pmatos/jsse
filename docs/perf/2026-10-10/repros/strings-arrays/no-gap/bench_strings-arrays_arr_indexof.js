// STATUS: NO GAP BEYOND BASELINE (global ratio 1.67x = 1.27x the scalar-loop control). Kept as a control / absolute-cost case, not as a builtin-gap claim.
// Array.prototype.indexOf / includes linear scans on a 5000-element dense array.
// Models: membership tests in Babylon / pdfjs / FlightPlanner.
// Cause: per-element i.to_string() + HasProperty + Get loop (array.rs:1115, :1215); absolute cost is high (~200 ns/element) but boa's is similar (~120 ns), so no gap beyond baseline.
// Measured (jsse v0.11.0 @ 2b621c5 vs boa 0.22.0, median of 5, buildbox3, 2026-10-10): global-scope jsse 4.13 s / boa 2.47 s = 1.67x; function-scope variant (fn-scope/) jsse 4.96 s / boa 2.78 s = 1.79x.
// Prints one checksum line; jsse and boa print the same line. Deterministic (LCG, no Date/Math.random).
// Global-scope variant (top-level `var`): isolates the builtin cost; the scalar-loop control (ctl_scalar_global) defines the baseline ratio here.
var a = [];
for (var i = 0; i < 5000; i++) a.push(i);
var acc = 0;
for (var r = 0; r < 4000; r++) {
  acc += a.indexOf(4999 - (r % 10));
  if (a.includes(2500 + r)) acc++;
}
console.log(acc);
