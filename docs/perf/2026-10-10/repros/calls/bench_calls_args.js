// Calls with `arguments` usage and default-parameter shapes.
// Measured 2026-10-10 on buildbox3 (jsse 2b621c5 v0.11.0 vs boa 0.22.0, median of 5): jsse 3.78 s / boa 0.92 s = 4.12x; jsse --no-bytecode 5.19 s; loadavg 3.5.
// Models: older ES5 code in JetStream (crypto, Box2D, early-boyer) using arguments.length/[i].
function sumArgs() {
  var s = 0;
  for (var i = 0; i < arguments.length; i++) s += arguments[i];
  return s;
}
function withDefault(a, b = 2) { return a + b; }
function main() {
  var t = 0;
  for (var i = 0; i < 300000; i++) {
    t += sumArgs(i, 1, 2) + withDefault(i);
  }
  return t;
}
console.log(main());
