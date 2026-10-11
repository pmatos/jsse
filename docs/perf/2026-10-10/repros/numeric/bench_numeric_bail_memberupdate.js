// Theme: numeric. Bytecode-eligibility cliff, variant "memberupdate": the hot loop below
// is the same in every bench_numeric_bail_*.js file. The only difference is one
// statement that NEVER EXECUTES but whose syntax (a `++` on a member expression) the bytecode compiler
// rejects (CompileError::Unsupported "update target"), so the whole function body
// falls back to the tree-walker. Models Octane crypto montReduce (86% of tree-walker work), Box2D (164 bodies), gbemu (167 bodies).
// Measured (buildbox3, 2026-10-10, jsse HEAD 2b621c5 / boa 0.22.0, median of 5): jsse 3.73 s, boa 0.67 s, ratio 5.57x, jsse --no-bytecode 3.78 s.
function kernel(n, o) {
  var s = 0, x = 1;
  if (n < 0) o.t++;
  for (var i = 0; i < n; i++) {
    x = (x * 1103515245 + 12345) & 0x7fffffff;
    s = (s + (x >> 7) + (i & 15)) | 0;
    s = s ^ (x >>> 3);
    if (s > 100000000) s = s - 100000000;
  }
  return s;
}
console.log("bail_memberupdate", kernel(1000000, { t: 0 }));
