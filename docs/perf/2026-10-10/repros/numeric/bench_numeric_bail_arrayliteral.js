// Theme: numeric. Bytecode-eligibility cliff, variant "arrayliteral": the hot loop below
// is the same in every bench_numeric_bail_*.js file. The only difference is one
// statement that NEVER EXECUTES but whose syntax (an array literal) the bytecode compiler
// rejects (CompileError::Unsupported "expression:Array"), so the whole function body
// falls back to the tree-walker. Models stanford-crypto-sha256/pbkdf2 (50% of tree-walker work), gbemu.
// Measured (buildbox3, 2026-10-10, jsse HEAD 2b621c5 / boa 0.22.0, median of 5): jsse 3.73 s, boa 0.57 s, ratio 6.54x, jsse --no-bytecode 3.73 s.
function kernel(n, o) {
  var s = 0, x = 1;
  var tbl = [1, 2, 3];
  for (var i = 0; i < n; i++) {
    x = (x * 1103515245 + 12345) & 0x7fffffff;
    s = (s + (x >> 7) + (i & 15)) | 0;
    s = s ^ (x >>> 3);
    if (s > 100000000) s = s - 100000000;
  }
  return s;
}
console.log("bail_arrayliteral", kernel(1000000, { t: 0 }));
