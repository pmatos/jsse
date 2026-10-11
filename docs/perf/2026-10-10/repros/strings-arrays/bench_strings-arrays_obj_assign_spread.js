// Object.assign({}, base) and {...base, extra}, 20-property source, 30k iterations.
// Models: immutable-update idioms (object spread) in Babylon/ML/mobx.
// Cause: same per-key descriptor reification + proxy_set on the target for each key.
// Measured (jsse v0.11.0 @ 2b621c5 vs boa 0.22.0, median of 5, buildbox3, 2026-10-10): global-scope jsse 2.98 s / boa 0.97 s = 3.07x; function-scope variant (fn-scope/) jsse 3.29 s / boa 1.04 s = 3.15x.
// Prints one checksum line; jsse and boa print the same line. Deterministic (LCG, no Date/Math.random).
// Global-scope variant (top-level `var`): isolates the builtin cost; the scalar-loop control (ctl_scalar_global) defines the baseline ratio here.
var base = {};
for (var i = 0; i < 20; i++) base["p" + i] = i;
var acc = 0;
for (var r = 0; r < 30000; r++) {
  var c = Object.assign({}, base);
  var d = { ...base, extra: r };
  acc += c.p3 + d.extra;
}
console.log(acc);
