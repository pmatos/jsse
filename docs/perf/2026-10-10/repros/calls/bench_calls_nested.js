// Calls from inside a function whose callees live 2-3 scope levels up (module-pattern IIFE):
// Measured 2026-10-10 on buildbox3 (jsse 2b621c5 v0.11.0 vs boa 0.22.0, median of 5): jsse 2.63 s / boa 0.87 s = 3.02x; jsse --no-bytecode 3.73 s; loadavg 8.3.
// every callee/variable resolution walks function -> IIFE -> global.
// Models: JetStream code wrapped in IIFEs/modules (bundled libraries, Babylon, pdfjs).
var result = (function () {
  var K = 7;
  function step(x) { return (x * 31 + K) & 1023; }
  function run(n) {
    var h = 1;
    for (var i = 0; i < n; i++) {
      h = step(h) + step(i);
    }
    return h;
  }
  return run(800000);
})();
console.log(result);
