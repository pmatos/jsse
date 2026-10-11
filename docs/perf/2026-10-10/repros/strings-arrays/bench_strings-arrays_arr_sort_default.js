// default sort() of 5 x 30k short strings.
// Models: sorting of names/keys.
// Cause: to_string_value() + Rust String cmp executed inside the comparator (2 allocations per comparison); UTF-8 byte order is also not UTF-16 code-unit order (conformance bug).
// Measured (jsse v0.11.0 @ 2b621c5 vs boa 0.22.0, median of 5, buildbox3, 2026-10-10): global-scope jsse 5.24 s / boa 2.88 s = 1.82x; function-scope variant (fn-scope/) jsse 5.18 s / boa 2.01 s = 2.58x.
// Prints one checksum line; jsse and boa print the same line. Deterministic (LCG, no Date/Math.random).
// Global-scope variant (top-level `var`): isolates the builtin cost; the scalar-loop control (ctl_scalar_global) defines the baseline ratio here.
var seed = 11;
function rnd() { seed = (seed * 1664525 + 1013904223) >>> 0; return seed; }
var acc = 0;
for (var r = 0; r < 20; r++) {
  var s = [];
  for (var i = 0; i < 30000; i++) s.push("k" + (rnd() % 100000));
  s.sort();
  acc += s[0].length + s[s.length - 1].length;
}
console.log(acc);
