// Theme: numeric. CONTROL for the bench_numeric_bail_*.js family: the hot loop below
// is identical in every file of the family; this variant has no unsupported
// construct, so the whole function compiles to bytecode and runs on the VM.
// The other variants add one never-executed statement the compiler rejects, which
// moves the entire function to the tree-walker (1.8x slower, see their headers).
// Models: any hot numeric function (LCG + shifts + masked adds), the loop shape of
// stanford-crypto / mandreel register code.
// Measured (buildbox3, 2026-10-10, jsse HEAD 2b621c5 / boa 0.22.0, median of 5): jsse 2.07 s, boa 0.62 s, ratio 3.34x, jsse --no-bytecode 3.73 s.
function kernel(n, o) {
  var s = 0, x = 1;
  for (var i = 0; i < n; i++) {
    x = (x * 1103515245 + 12345) & 0x7fffffff;
    s = (s + (x >> 7) + (i & 15)) | 0;
    s = s ^ (x >>> 3);
    if (s > 100000000) s = s - 100000000;
  }
  return s;
}
console.log("bail_none", kernel(1000000, { t: 0 }));
