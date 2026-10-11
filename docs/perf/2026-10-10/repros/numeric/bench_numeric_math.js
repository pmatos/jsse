// Theme: numeric. Calls to Math builtins (floor, sqrt, abs, min, max, imul)
// from a hot loop, results fed back into integer/float arithmetic. Models
// Box2D (Math.sqrt/abs/min/max in the solver), navier-stokes, raytrace-style
// vector math and the Math.imul/Math.floor idioms of asm.js-style output.
// Isolates: native-function call overhead from bytecode (LoadMethod on the
// global `Math` + Call to a native). The calls agent owns generic call cost;
// this file only shows the per-native-call share of numeric kernels.
// Measured (buildbox3, 2026-10-10, jsse HEAD 2b621c5 / boa 0.22.0, median of 5): jsse 2.93 s, boa 1.37 s, ratio 2.14x, jsse --no-bytecode 4.58 s.
function kernel(n) {
  var s = 0, x = 0.5, y = 1.5;
  for (var i = 0; i < n; i++) {
    x = Math.abs(x - 0.3) + 0.01;
    y = Math.sqrt(y * y + x);
    s = (s + Math.floor(y * 8)) | 0;
    s = (s + Math.imul(s, 31) + Math.min(i, 7) + Math.max(i & 3, 1)) | 0;
    if (y > 1000) y = 1.5;
  }
  return s;
}
console.log("math", kernel(500000));
