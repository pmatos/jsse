// Theme: numeric. Typed-array element read/write traffic from a hot loop, the
// Emscripten/Mandreel "heap32[(fp + k) >> 2]" shape plus Uint8Array byte
// reads and a Float64Array accumulate. Typed arrays are module-level `var`s
// (global lookups) but all indices and temporaries are locals.
// Models: mandreel (heap32/heap8 traffic, typed arrays are global vars), gbemu (memory[] reads/writes),
// stanford-crypto (word arrays), bench_opmix "elem" phase.
// Isolates: GetElement/SetElement on typed arrays incl. numeric_index_fast_get/set.
// Measured (buildbox3, 2026-10-10, jsse HEAD 2b621c5 / boa 0.22.0, median of 5): jsse 2.78 s, boa 1.42 s, ratio 1.96x, jsse --no-bytecode 5.09 s.
var heap32 = new Int32Array(1 << 14);
var heap8 = new Uint8Array(1 << 14);
var f64 = new Float64Array(256);

function work(n) {
  var fp = 64, acc = 0, seed = 12345;
  for (var i = 0; i < n; i++) {
    seed = (seed * 1103515245 + 12345) & 0x7fffffff;
    var j = (seed >> 8) & 1023;
    var r0 = heap32[fp + (j & 63)];
    r0 = (r0 + 4 + j) | 0;
    heap32[fp + (j & 63)] = r0;
    var r1 = heap32[fp + 1];
    r1 = (r0 + r1) | 0;
    heap32[fp + 1] = r1 & 65535;
    heap8[j] = r1;
    acc = (acc + heap8[(j + 1) & 1023] + heap8[j]) | 0;
    f64[j & 255] = f64[(j + 1) & 255] + 0.5;
  }
  return (acc + heap32[fp + 1] + f64[7]) | 0;
}
console.log("typedarray_global", work(400000));
