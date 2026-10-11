// STATUS: NO GAP BEYOND BASELINE (global ratio 1.48x = 1.13x the scalar-loop control). Kept as a control / absolute-cost case, not as a builtin-gap claim.
// CONTROL: computed string-keyed property writes/reads (`o["key" + i]`) on a 200k-key object.
// Models: dictionary-style objects (hash-map JetStream benchmark).
// Cause: CONTROL: dictionary-style string-keyed property access sits at the scalar-loop baseline.
// Measured (jsse v0.11.0 @ 2b621c5 vs boa 0.22.0, median of 5, buildbox3, 2026-10-10): global-scope jsse 2.93 s / boa 1.97 s = 1.48x; function-scope variant (fn-scope/) jsse 2.58 s / boa 0.79 s = 3.28x.
// Prints one checksum line; jsse and boa print the same line. Deterministic (LCG, no Date/Math.random).
// Global-scope variant (top-level `var`): isolates the builtin cost; the scalar-loop control (ctl_scalar_global) defines the baseline ratio here.
var o = {};
for (var i = 0; i < 200000; i++) { o["key" + i] = i; }
var sum = 0;
for (var r = 0; r < 3; r++) {
  for (var i = 0; i < 200000; i++) { sum += o["key" + i]; }
}
console.log(sum);
