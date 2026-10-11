// Closure creation where the closure body is moderately large (~25 statements).
// Measured 2026-10-10 on buildbox3 (jsse 2b621c5 v0.11.0 vs boa 0.22.0, median of 5): jsse 3.93 s / boa 0.67 s = 5.89x; jsse --no-bytecode 3.73 s; loadavg 6.4.
// Each makeScorer() call creates a fresh function object from the SAME function
// expression, and calls it once. Exposes any per-closure-object work that scales
// with body size (e.g. re-compiling the body to bytecode for every new closure).
// Models: callbacks/lambdas created in hot paths (JetStream lazy-collections,
// doxbee-promise/async, js-tokens, pdfjs).
function makeScorer(bias) {
  return function (a, b, c) {
    var s = bias;
    if (a > b) { s += a - b; } else { s += b - a; }
    if (b > c) { s += b - c; } else { s += c - b; }
    if (a > c) { s += a - c; } else { s += c - a; }
    s = s + a * 2 + b * 3 + c * 5;
    s = s - (a & 3) - (b & 7) - (c & 15);
    s = s + (a | 1) + (b | 2) + (c | 4);
    s = s ^ (a << 1);
    s = s ^ (b >> 1);
    s = s + (a < b ? 1 : 2) + (b < c ? 3 : 4) + (a < c ? 5 : 6);
    var t = s % 1021;
    t = t + a % 7 + b % 11 + c % 13;
    return (s + t) | 0;
  };
}
function main() {
  var total = 0;
  for (var i = 0; i < 150000; i++) {
    var f = makeScorer(i & 31);
    total = (total + f(i & 63, (i >> 2) & 63, (i >> 4) & 63)) | 0;
  }
  return total;
}
console.log(main());
