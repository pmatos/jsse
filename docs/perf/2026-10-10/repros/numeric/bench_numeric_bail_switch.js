// Theme: numeric. Bytecode-eligibility cliff, variant "switch": the hot loop below
// is the same in every bench_numeric_bail_*.js file. The only difference is one
// statement that NEVER EXECUTES but whose syntax (a `switch` statement) the bytecode compiler
// rejects (CompileError::Unsupported "statement:Switch"), so the whole function body
// falls back to the tree-walker. Models gbemu's opcode-dispatch switch (~50% of tree-walker work) and Box2D (~82%).
// Measured (buildbox3, 2026-10-10, jsse HEAD 2b621c5 / boa 0.22.0, median of 5): jsse 3.68 s, boa 0.62 s, ratio 5.94x, jsse --no-bytecode 3.68 s.
function kernel(n, o) {
  var s = 0, x = 1;
  switch (n) { case -1: s = 1; break; }
  for (var i = 0; i < n; i++) {
    x = (x * 1103515245 + 12345) & 0x7fffffff;
    s = (s + (x >> 7) + (i & 15)) | 0;
    s = s ^ (x >>> 3);
    if (s > 100000000) s = s - 100000000;
  }
  return s;
}
console.log("bail_switch", kernel(1000000, { t: 0 }));
