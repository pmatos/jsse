// Object.keys() on a 20-property object, 60k calls.
// Models: object iteration in pdfjs, Babylon, ML, mobx/proxy benchmarks.
// Cause: spec-literal path: proxy_own_keys -> JsString -> JsPropertyKey -> proxy_get_own_property_descriptor materialises a descriptor JS object per key, then reads `enumerable` back.
// Measured (jsse v0.11.0 @ 2b621c5 vs boa 0.22.0, median of 5, buildbox3, 2026-10-10): global-scope jsse 4.64 s / boa 0.52 s = 8.96x; function-scope variant (fn-scope/) jsse 4.58 s / boa 0.32 s = 14.12x.
// Prints one checksum line; jsse and boa print the same line. Deterministic (LCG, no Date/Math.random).
// Function-scope variant: realistic (JetStream code runs inside functions) but includes jsse's ~4x local-variable overhead (see ctl_scalar_fn), which belongs to another theme.
(function () {
var base = {};
for (var i = 0; i < 20; i++) base["p" + i] = i;
var acc = 0;
for (var r = 0; r < 60000; r++) { acc += Object.keys(base).length; }
console.log(acc);
})();
