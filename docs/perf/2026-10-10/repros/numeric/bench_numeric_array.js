// Theme: numeric. Plain Array<number> element read and write (not typed
// arrays) from a hot loop: stencil sweeps over a pre-filled dense array with
// computed integer indices, the access pattern of navier-stokes (x[i] = ...
// x[++currentRow]), raytrace, crypto's BigInteger limb arrays and ai-astar grids.
// No calls inside the loop, everything is function-local.
// Isolates: GetElement/SetElement on ordinary Arrays (no numeric fast path for
// stores, string-key detour; read path hashes the decimal index string).
// Measured (buildbox3, 2026-10-10, jsse HEAD 2b621c5 / boa 0.22.0, median of 5): jsse 3.78 s, boa 1.62 s, ratio 2.33x, jsse --no-bytecode 6.19 s.
function sweep(x, y, n, passes) {
  var s = 0;
  for (var p = 0; p < passes; p++) {
    for (var i = 1; i < n - 1; i++) {
      y[i] = (x[i - 1] + x[i] + x[i + 1]) * 0.25;
    }
    for (var i = 1; i < n - 1; i++) {
      x[i] = y[i] * 0.5 + x[i] * 0.5;
    }
    s += x[n >> 1];
  }
  return s;
}

var n = 4096;
var x = new Array(n), y = new Array(n);
var seed = 7;
for (var i = 0; i < n; i++) {
  seed = (seed * 1103515245 + 12345) & 0x7fffffff;
  x[i] = (seed & 1023) / 8;
  y[i] = 0;
}
var r = sweep(x, y, n, 300);
console.log("array", r.toFixed(6), x[17].toFixed(6));
