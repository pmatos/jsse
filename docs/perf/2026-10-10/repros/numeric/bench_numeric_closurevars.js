// Theme: numeric. Hot loop that reads variables of the *enclosing* function
// (closure / module-pattern state) plus plain Array stencil traffic: the
// FluidField shape of JetStream Octane/navier-stokes, where lin_solve/set_bnd
// read `width`, `height`, `rowSize`, `iterations` from the outer function on
// every iteration. Also models Octane crypto/Box2D module-level helpers.
// Isolates: free-variable (non-local, scope-chain) identifier resolution.
// Measured (buildbox3, 2026-10-10, jsse HEAD 2b621c5 / boa 0.22.0, median of 5): jsse 2.78 s, boa 1.32 s, ratio 2.11x, jsse --no-bytecode 6.54 s.
function FluidField() {
  var width = 62, height = 62, rowSize = width + 2, size = rowSize * (height + 2);
  var iterations = 10;
  var dens = new Array(size), prev = new Array(size);

  function reset() {
    for (var i = 0; i < size; i++) { dens[i] = (i % 7) * 0.5; prev[i] = 0; }
  }

  function linSolve(x, x0, a, c) {
    var invC = 1 / c;
    for (var k = 0; k < iterations; k++) {
      for (var j = 1; j <= height; j++) {
        var currentRow = j * rowSize;
        var lastRow = (j - 1) * rowSize;
        var nextRow = (j + 1) * rowSize;
        var lastX = x[currentRow];
        ++currentRow;
        for (var i = 1; i <= width; i++) {
          lastX = x[currentRow] = (x0[currentRow] + a * (lastX + x[++currentRow] + x[++lastRow] + x[++nextRow])) * invC;
        }
      }
    }
  }

  function run(frames) {
    reset();
    for (var f = 0; f < frames; f++) {
      linSolve(dens, prev, 0.0001, 1 + 4 * 0.0001);
      var t = prev; prev = dens; dens = t;
    }
    var s = 0;
    for (var i = 0; i < size; i += 13) s += dens[i];
    return s;
  }
  return { run: run };
}
console.log("closurevars", FluidField().run(30).toFixed(6));
