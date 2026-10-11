// STATUS: NO GAP BEYOND BASELINE (global ratio 1.58x = 1.20x the scalar-loop control). Kept as a control / absolute-cost case, not as a builtin-gap claim.
// for (var v of a) over a 100k-element array, 12 passes.
// Models: UniPoker (`for (let player of players)`), modern-JS loops everywhere.
// Cause: for-of drives the generic iterator protocol (callgrind: ~18.4k instructions per element, 77% in exec_for_of, iterator_next 27.6%) but boa pays a similar cost at this scale, so the ratio to boa sits at the baseline; absolute-cost case for WI-5.
// Measured (jsse v0.11.0 @ 2b621c5 vs boa 0.22.0, median of 5, buildbox3, 2026-10-10): global-scope jsse 3.98 s / boa 2.53 s = 1.58x; function-scope variant (fn-scope/) jsse 3.66 s / boa 1.20 s = 3.06x.
// Prints one checksum line; jsse and boa print the same line. Deterministic (LCG, no Date/Math.random).
// Global-scope variant (top-level `var`): isolates the builtin cost; the scalar-loop control (ctl_scalar_global) defines the baseline ratio here.
var a = [];
for (var i = 0; i < 100000; i++) a.push(i);
var s = 0;
for (var r = 0; r < 12; r++) { for (var v of a) s += v; }
console.log(s);
