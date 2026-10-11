// Recursive fib: pure call/return overhead (1 arg, global function, no closure state).
// Measured 2026-10-10 on buildbox3 (jsse 2b621c5 v0.11.0 vs boa 0.22.0, median of 5): jsse 3.63 s / boa 1.17 s = 3.10x; jsse --no-bytecode 4.08 s; loadavg 11.1.
// Models: bench_fib.js; call-heavy core of Octane richards/deltablue/raytrace.
function fib(n) {
  if (n < 2) return n;
  return fib(n - 1) + fib(n - 2);
}
console.log(fib(30));
