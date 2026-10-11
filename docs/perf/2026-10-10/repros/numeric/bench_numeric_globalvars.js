// Theme: numeric. Hot loop whose free variables are *script-level* `var`s
// (properties of the global object): the Mandreel/Emscripten output shape
// (`heap32`, `heap8`, `heapFloat`, `__FUNCTION_TABLE__` and register-spill
// globals are all top-level vars read by every generated function) and old
// Octane-style code. Locals are only the loop temporaries.
// Isolates: global-variable load/store cost (resolve_identifier global-object path),
// separately from element access: contrast with bench_numeric_typedarray_local.js.
// Measured (buildbox3, 2026-10-10, jsse HEAD 2b621c5 / boa 0.22.0, median of 5): jsse 3.18 s, boa 1.77 s, ratio 1.80x, jsse --no-bytecode 3.98 s.
var g_a = 1, g_b = 2, g_c = 3;
var g_limit = 65535;
var g_heap = new Int32Array(256);

function step(n) {
  var s = 0;
  for (var i = 0; i < n; i++) {
    g_a = (g_a + g_b) & g_limit;
    g_b = (g_b + g_c + i) & g_limit;
    g_c = (g_c ^ g_a) & g_limit;
    s = (s + g_heap[i & 255] + g_c) | 0;
    g_heap[i & 255] = g_a;
  }
  return s;
}
console.log("globalvars", step(500000), g_a, g_b, g_c);
