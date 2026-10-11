// sort(function (x, y) { return x - y; }) on 5 x 50k-element arrays.
// Models: UniPoker `_cards.sort(...)`, ML, pdfjs.
// Cause: comparator call overhead (call_function + vec![x,y] + to_number) plus index-string gather/scatter around the sort.
// Measured (jsse v0.11.0 @ 2b621c5 vs boa 0.22.0, median of 5, buildbox3, 2026-10-10): global-scope jsse 5.64 s / boa 2.27 s = 2.48x; function-scope variant (fn-scope/) jsse 6.15 s / boa 1.91 s = 3.22x.
// Prints one checksum line; jsse and boa print the same line. Deterministic (LCG, no Date/Math.random).
// Function-scope variant: realistic (JetStream code runs inside functions) but includes jsse's ~4x local-variable overhead (see ctl_scalar_fn), which belongs to another theme.
(function () {
var seed = 11;
function rnd() { seed = (seed * 1664525 + 1013904223) >>> 0; return seed; }
var acc = 0;
for (var r = 0; r < 5; r++) {
  var a = [];
  for (var i = 0; i < 50000; i++) a.push(rnd() % 1000000);
  a.sort(function (x, y) { return x - y; });
  acc += a[0] + a[a.length - 1];
}
console.log(acc);
})();
