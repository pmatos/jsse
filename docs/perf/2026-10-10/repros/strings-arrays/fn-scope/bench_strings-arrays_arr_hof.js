// map / filter / forEach / reduce with small closures over 100k-element arrays, 10 rounds.
// Models: ML (matrix maps), Babylon, FlightPlanner, pdfjs.
// Cause: spec-literal element loop: k.to_string() + obj_has_throw + obj_get + call_function(vec![..]) per element, plus per-call env creation.
// Measured (jsse v0.11.0 @ 2b621c5 vs boa 0.22.0, median of 5, buildbox3, 2026-10-10): global-scope jsse 4.03 s / boa 1.72 s = 2.34x; function-scope variant (fn-scope/) jsse 4.31 s / boa 1.55 s = 2.78x.
// Prints one checksum line; jsse and boa print the same line. Deterministic (LCG, no Date/Math.random).
// Function-scope variant: realistic (JetStream code runs inside functions) but includes jsse's ~4x local-variable overhead (see ctl_scalar_fn), which belongs to another theme.
(function () {
var a = [];
for (var i = 0; i < 100000; i++) a.push(i);
var t = 0;
for (var r = 0; r < 10; r++) {
  var m = a.map(function (x) { return x * 2; });
  var f = m.filter(function (x) { return x % 3 === 0; });
  var s = 0;
  f.forEach(function (x) { s += x; });
  t += s + f.reduce(function (p, c) { return p + c; }, 0);
}
console.log(t);
})();
