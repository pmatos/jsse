// STATUS: NO GAP BEYOND BASELINE (global ratio 1.40x = 1.06x the scalar-loop control). Kept as a control / absolute-cost case, not as a builtin-gap claim.
// push x 600k, indexed read loop, indexed read-modify-write loop, pop x 300k.
// Models: generic dense-array loops.
// Cause: push has a dense fast path (array.rs:886); ratio is at the scalar-loop baseline, no builtin-specific gap measured.
// Measured (jsse v0.11.0 @ 2b621c5 vs boa 0.22.0, median of 5, buildbox3, 2026-10-10): global-scope jsse 4.93 s / boa 3.53 s = 1.40x; function-scope variant (fn-scope/) jsse 3.84 s / boa 1.09 s = 3.51x.
// Prints one checksum line; jsse and boa print the same line. Deterministic (LCG, no Date/Math.random).
// Function-scope variant: realistic (JetStream code runs inside functions) but includes jsse's ~4x local-variable overhead (see ctl_scalar_fn), which belongs to another theme.
(function () {
var a = [];
for (var i = 0; i < 600000; i++) a.push(i);
var s = 0;
for (var i = 0; i < a.length; i++) s += a[i];
for (var i = 0; i < a.length; i++) a[i] = a[i] ^ 5;
while (a.length > 300000) a.pop();
console.log(s, a.length, a[299999]);
})();
