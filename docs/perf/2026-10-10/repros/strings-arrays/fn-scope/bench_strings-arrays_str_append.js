// Repeated `s += "x"` building a 200k-char string (global var).
// Models: string building loops (pdfjs Type1/Flate output, Babylon codegen, code-load).
// Cause: JsString is a flat Arc<Vec<u16>>; every `+`/`+=` copies the whole left operand (RC>=2 so Arc::try_unwrap fails). Quadratic: 2x N -> ~4x time.
// Measured (jsse v0.11.0 @ 2b621c5 vs boa 0.22.0, median of 5, buildbox3, 2026-10-10): global-scope jsse 3.53 s / boa 1.37 s = 2.58x; function-scope variant (fn-scope/) jsse 3.60 s / boa 1.04 s = 3.46x.
// Prints one checksum line; jsse and boa print the same line. Deterministic (LCG, no Date/Math.random).
// Function-scope variant: realistic (JetStream code runs inside functions) but includes jsse's ~4x local-variable overhead (see ctl_scalar_fn), which belongs to another theme.
(function () {
var s = "";
for (var i = 0; i < 200000; i++) { s += "x"; }
console.log(s.length, s.charCodeAt(199999));
})();
