// Closure creation + one call each (factory pattern). bench_closures.js scaled up
// Measured 2026-10-10 on buildbox3 (jsse 2b621c5 v0.11.0 vs boa 0.22.0, median of 5): jsse 2.83 s / boa 0.57 s = 4.98x; jsse --no-bytecode 2.98 s; loadavg 5.0.
// with the loop inside a function. Every iteration allocates a fresh function
// object (+ its .prototype object) from the same function-expression AST.
// Models: Octane/delta-blue, closure-heavy callbacks, JetStream lazy-collections.
function makeAdder(x) {
  return function (y) { return x + y; };
}
function main() {
  var sum = 0;
  for (var i = 0; i < 300000; i++) {
    var add5 = makeAdder(i & 15);
    sum += add5(i & 255);
  }
  return sum;
}
console.log(main());
