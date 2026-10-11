// Leaf calls: call a trivial 2-arg global function in a hot loop (no recursion,
// Measured 2026-10-10 on buildbox3 (jsse 2b621c5 v0.11.0 vs boa 0.22.0, median of 5): jsse 3.68 s / boa 1.17 s = 3.15x; jsse --no-bytecode 5.64 s; loadavg 9.8.
// call depth 1). Isolates per-call setup/teardown (env + this/arguments bindings,
// call frame push/pop, GC root push/pop, IC probe) from recursion effects.
// The loop lives inside a function so loop variables are locals, not global-object
// properties (global-scope variable access is measured separately in
// bench_calls_global_loop.js).
// Models: small helper calls in richards/deltablue/raytrace/crypto.
function add(a, b) { return a + b; }
function main() {
  var s = 0;
  for (var i = 0; i < 2000000; i++) {
    s = add(s, i & 7);
  }
  return s;
}
console.log(main());
