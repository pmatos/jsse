// indexOf(needle, pos) forward scan (2291 hits) plus 1500 indexOf(needle, start) calls that hit within a few words, on a 98 KB string.
// Models: tokenizers/lexers that walk a long source with indexOf (js-tokens, pdfjs Lexer, Babylon).
// Cause: every call round-trips the receiver through a Rust String (utf16->utf8->utf16): O(n) per call regardless of the match position.
// Measured (jsse v0.11.0 @ 2b621c5 vs boa 0.22.0, median of 5, buildbox3, 2026-10-10): global-scope jsse 2.88 s / boa 0.27 s = 10.80x; function-scope variant (fn-scope/) jsse 3.24 s / boa 0.23 s = 14.38x.
// Prints one checksum line; jsse and boa print the same line. Deterministic (LCG, no Date/Math.random).
// Function-scope variant: realistic (JetStream code runs inside functions) but includes jsse's ~4x local-variable overhead (see ctl_scalar_fn), which belongs to another theme.
(function () {
var seed = 1;
function rnd() { seed = (seed * 1664525 + 1013904223) >>> 0; return seed; }
var words = [];
for (var i = 0; i < 20000; i++) words.push("w" + (rnd() % 1000));
var text = words.join(" ");
var n = 0, acc = 0;
var pos = 0;
while ((pos = text.indexOf("w1", pos)) !== -1) { n++; pos++; }
for (var r = 0; r < 1500; r++) { acc += text.indexOf("w9", 50000 + r); }
console.log(text.length, n, acc);
})();
