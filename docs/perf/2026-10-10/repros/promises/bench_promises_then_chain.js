// Isolates per-.then() cost with a SHALLOW microtask queue: one chain of
// N*K `.then()`s built synchronously, drained at queue depth 1. Same number of
// reactions/jobs as bench_promises_queue_deep.js (N*K), so the ratio of the two
// exposes any queue-depth-dependent cost.
// Measured (buildbox3, median of 5, jsse HEAD 2b621c5 v0.11.0 vs boa 0.22.0, 2026-10-10, loadavg 8.5):
//   jsse 4.58 s, boa 1.02 s, ratio 4.5x; jsse --no-bytecode 4.43 s.
//   Scaling (TOTAL=60k/120k/240k thens, single runs, load ~3.5): jsse 1.76/4.51/14.17 s, boa 0.49/1.00/1.90 s.
const TOTAL = 20000 * 6;
let p = Promise.resolve(0);
for (let i = 0; i < TOTAL; i++) p = p.then(v => v + 1);
p.then(v => console.log("then-chain value=" + v));
