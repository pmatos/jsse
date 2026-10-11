// STATUS: NO GAP BEYOND BASELINE (global ratio 1.38x = 1.05x the scalar-loop control). Kept as a control / absolute-cost case, not as a builtin-gap claim.
// slice(a,b), slice(0), concat([..]) on a 5000-element array.
// Models: UniPoker `CardDeck._newDeck.slice(0)` (hot), generic array copies.
// Cause: element-by-element Get/CreateDataProperty with string keys (array.rs:1380-1560; slice(0) profile: create_data_property_or_throw 24% inclusive, ~2.4k instructions/element); boa is equally slow here, so no gap beyond baseline.
// Measured (jsse v0.11.0 @ 2b621c5 vs boa 0.22.0, median of 5, buildbox3, 2026-10-10): global-scope jsse 7.84 s / boa 5.69 s = 1.38x; function-scope variant (fn-scope/) jsse 8.66 s / boa 5.70 s = 1.52x.
// Prints one checksum line; jsse and boa print the same line. Deterministic (LCG, no Date/Math.random).
// Function-scope variant: realistic (JetStream code runs inside functions) but includes jsse's ~4x local-variable overhead (see ctl_scalar_fn), which belongs to another theme.
(function () {
var a = [];
for (var i = 0; i < 5000; i++) a.push(i);
var acc = 0;
for (var r = 0; r < 1800; r++) {
  acc += a.slice(10, 400).length;
  acc += a.concat([1, 2, 3]).length;
  acc += a.slice(0).length;
}
console.log(acc);
})();
