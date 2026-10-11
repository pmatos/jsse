// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3 at loadavg ~7, median of 5: jsse 3.03 s, boa 0.47 s, ratio 6.49x; jsse --no-bytecode 3.08 s (median of 3).
// Exercises: driving a generator by hand with it.next()/it.next(v) in a loop
// (no for-of): measures the raw call -> resume -> iterator-result path.
function* acc() {
  let total = 0;
  while (true) {
    const v = yield total;
    total = (total + v) | 0;
  }
}
const it = acc();
it.next();
let last = 0;
for (let i = 0; i < 400000; i++) last = it.next(i).value;
console.log("manual_next", last);
