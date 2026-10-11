// `t = t + "ab" + j` (left-assoc chain: two full copies per iteration), 50k iterations.
// Models: string accumulation with mixed number/string operands.
// Cause: same flat-string copy as str_append, twice per statement.
// Measured (jsse v0.11.0 @ 2b621c5 vs boa 0.22.0, median of 5, buildbox3, 2026-10-10): global-scope jsse 3.53 s / boa 0.97 s = 3.64x; function-scope variant (fn-scope/) jsse 3.45 s / boa 0.94 s = 3.66x.
// Prints one checksum line; jsse and boa print the same line. Deterministic (LCG, no Date/Math.random).
// Global-scope variant (top-level `var`): isolates the builtin cost; the scalar-loop control (ctl_scalar_global) defines the baseline ratio here.
var t = "";
for (var j = 0; j < 50000; j++) { t = t + "ab" + j; }
console.log(t.length, t.charCodeAt(12345));
