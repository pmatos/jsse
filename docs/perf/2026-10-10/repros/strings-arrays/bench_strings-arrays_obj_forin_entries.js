// for-in + hasOwnProperty over a 20-property object and Object.entries(), 20k iterations.
// Models: object iteration in pdfjs, Babylon.
// Cause: for-in key snapshot/enumerable checks per key through JsString<->JsPropertyKey conversions; hasOwnProperty goes through the full method lookup + ToPropertyKey; entries allocates a pair array per key.
// Measured (jsse v0.11.0 @ 2b621c5 vs boa 0.22.0, median of 5, buildbox3, 2026-10-10): global-scope jsse 4.08 s / boa 1.57 s = 2.60x; function-scope variant (fn-scope/) jsse 4.27 s / boa 1.15 s = 3.72x.
// Prints one checksum line; jsse and boa print the same line. Deterministic (LCG, no Date/Math.random).
// Global-scope variant (top-level `var`): isolates the builtin cost; the scalar-loop control (ctl_scalar_global) defines the baseline ratio here.
var base = {};
for (var i = 0; i < 20; i++) base["p" + i] = i;
var acc = 0;
for (var r = 0; r < 20000; r++) {
  for (var key in base) { if (base.hasOwnProperty(key)) acc++; }
  acc += Object.entries(base).length;
}
console.log(acc);
