// Isolates microtask-queue DEPTH: N independent promise chains of K `.then()`s
// each, all started synchronously, so the microtask queue holds ~N jobs at all
// times (this is the doxbee shape). Total jobs = N*K. Compare with
// bench_promises_then_chain.js, which runs the SAME number of jobs at queue depth 1.
// Models: doxbee-promise, doxbee-async (10k concurrent chains).
// Measured (buildbox3, median of 5, jsse HEAD 2b621c5 v0.11.0 vs boa 0.22.0, 2026-10-10, loadavg 9.9):
//   jsse 9.90 s, boa 1.07 s, ratio 9.3x; jsse --no-bytecode 10.06 s.
const N = 20000, K = 5;
let sum = 0;
for (let i = 0; i < N; i++) {
  let p = Promise.resolve(i);
  for (let k = 0; k < K; k++) p = p.then(v => v + 1);
  p.then(v => { sum = (sum + v) | 0; });
}
let ticks = 0;
(function wait() {
  if (++ticks < K + 3) { Promise.resolve().then(wait); return; }
  console.log("queue-deep sum=" + sum);
})();
