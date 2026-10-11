// STATUS: NO GAP BEYOND BASELINE (global ratio 1.61x = 1.22x the scalar-loop control). Kept as a control / absolute-cost case, not as a builtin-gap claim.
// 2-D grid stencil: g[i-1][j] + g[i+1][j] + g[i][j-1] + g[i][j+1] over 200x200, 12 passes.
// Models: gaussian-blur, navier-stokes style array-of-array code.
// Cause: 4 element reads per cell via numeric_index_fast_get; the ratio is at the scalar-loop baseline (variable/element access), no builtin-specific gap measured.
// Measured (jsse v0.11.0 @ 2b621c5 vs boa 0.22.0, median of 5, buildbox3, 2026-10-10): global-scope jsse 2.93 s / boa 1.82 s = 1.61x; function-scope variant (fn-scope/) jsse 2.42 s / boa 0.79 s = 3.08x.
// Prints one checksum line; jsse and boa print the same line. Deterministic (LCG, no Date/Math.random).
// Function-scope variant: realistic (JetStream code runs inside functions) but includes jsse's ~4x local-variable overhead (see ctl_scalar_fn), which belongs to another theme.
(function () {
var N = 200;
var g = [];
for (var i = 0; i < N; i++) { var row = []; for (var j = 0; j < N; j++) row.push((i * j) & 15); g.push(row); }
var s = 0;
for (var r = 0; r < 12; r++) {
  for (var i = 1; i < N - 1; i++) {
    for (var j = 1; j < N - 1; j++) { s += g[i - 1][j] + g[i + 1][j] + g[i][j - 1] + g[i][j + 1]; }
  }
}
console.log(s);
})();
