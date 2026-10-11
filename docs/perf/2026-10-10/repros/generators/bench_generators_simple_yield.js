// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3 at loadavg ~7, median of 5: jsse 5.43 s, boa 1.02 s, ratio 5.34x; jsse --no-bytecode 5.39 s (median of 3).
// Exercises: a plain `function*` with a counting loop consumed by for-of; one
// iterator-result object + one state-machine resume per element.
// Models: lazy-collections (range()), sync-fs (ls()), js-tokens (outer for-of).
// Isolates the per-resume cost of generator.next() with no delegation.
const N = 300000;
function* count(n) {
  for (let i = 0; i < n; i++) yield i;
}
let sum = 0;
for (let r = 0; r < 3; r++) {
  for (const v of count(N)) sum = (sum + v) | 0;
}
console.log("simple_yield", sum);
