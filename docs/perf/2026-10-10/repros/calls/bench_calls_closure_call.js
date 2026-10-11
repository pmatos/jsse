// Calling an ALREADY-created closure that reads a captured variable (2 scope hops)
// Measured 2026-10-10 on buildbox3 (jsse 2b621c5 v0.11.0 vs boa 0.22.0, median of 5): jsse 3.38 s / boa 1.12 s = 3.02x; jsse --no-bytecode 5.04 s; loadavg 4.9.
// and mutates a captured counter: isolates call cost + captured-variable lookup
// from closure creation. Loop inside a function (locals, not globals).
function makeCounter() {
  var count = 0;
  var step = 3;
  return function (d) { count += step + d; return count; };
}
function main() {
  var c = makeCounter();
  var r = 0;
  for (var i = 0; i < 1500000; i++) {
    r = c(i & 3);
  }
  return r;
}
console.log(main());
