// CONTROL/cross-cutting: same call as bench_calls_leaf.js but the loop and its
// Measured 2026-10-10 on buildbox3 (jsse 2b621c5 v0.11.0 vs boa 0.22.0, median of 5): jsse 1.72 s / boa 0.87 s = 1.98x; jsse --no-bytecode 1.57 s; loadavg 10.4.
// variables are script-level `var`s (global-object properties). Shows the cost of
// global-scope variable resolution on top of the call cost.
// Models: JetStream drivers/scripts whose hot loops run at top level.
function add(a, b) { return a + b; }
var s = 0;
for (var i = 0; i < 400000; i++) {
  s = add(s, i & 7);
}
console.log(s);
