// join(',') on 5000 numbers and join('') on 5000 strings, 1000 rounds each.
// Models: UniPoker `_cards.join("")`, pdfjs string assembly.
// Cause: per-element to_string + Rust String per part + UTF-8->UTF-16 conversion of the whole result.
// Measured (jsse v0.11.0 @ 2b621c5 vs boa 0.22.0, median of 5, buildbox3, 2026-10-10): global-scope jsse 2.63 s / boa 1.47 s = 1.79x; function-scope variant (fn-scope/) jsse 3.04 s / boa 1.66 s = 1.83x.
// Prints one checksum line; jsse and boa print the same line. Deterministic (LCG, no Date/Math.random).
// Function-scope variant: realistic (JetStream code runs inside functions) but includes jsse's ~4x local-variable overhead (see ctl_scalar_fn), which belongs to another theme.
(function () {
var a = [];
for (var i = 0; i < 5000; i++) a.push(i);
var acc = 0;
for (var r = 0; r < 1000; r++) { acc += a.join(",").length; }
var w = [];
for (var i = 0; i < 5000; i++) w.push("item" + i);
for (var r = 0; r < 1000; r++) { acc += w.join("").length; }
console.log(acc);
})();
