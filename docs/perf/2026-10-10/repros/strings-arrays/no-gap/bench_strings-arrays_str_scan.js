// STATUS: NO GAP BEYOND BASELINE (global ratio 1.70x = 1.29x the scalar-loop control). Kept as a control / absolute-cost case, not as a builtin-gap claim.
// charCodeAt(k) scan over a 270 KB string (tokenizer inner loop), 8 passes.
// Models: js-tokens, pdfjs Lexer, Babylon tokenizer.
// Cause: per-iteration cost is dominated by name resolution of the loop variables (callgrind: SipHash 32% of instructions, resolve_identifier/put_value_by_ref/find_binding_env); charCodeAt itself is cheap (this_js_string is an Arc clone). Same shape as ctl_scalar_*.
// Measured (jsse v0.11.0 @ 2b621c5 vs boa 0.22.0, median of 5, buildbox3, 2026-10-10): global-scope jsse 6.84 s / boa 4.03 s = 1.70x; function-scope variant (fn-scope/) jsse 5.43 s / boa 1.04 s = 5.23x.
// Prints one checksum line; jsse and boa print the same line. Deterministic (LCG, no Date/Math.random).
// Global-scope variant (top-level `var`): isolates the builtin cost; the scalar-loop control (ctl_scalar_global) defines the baseline ratio here.
var parts = [];
for (var i = 0; i < 5000; i++) parts.push("function foo" + i + "(a, b) { return a + b * " + i + "; }\n");
var src = parts.join("");
var sum = 0;
for (var r = 0; r < 8; r++) {
  for (var k = 0; k < src.length; k++) { sum = (sum + src.charCodeAt(k)) | 0; }
}
console.log(src.length, sum);
