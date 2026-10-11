// Theme: numeric. Same typed-array element traffic as
// bench_numeric_typedarray_global.js, but the arrays are function parameters
// (locals), so identifier resolution is the cheap same-scope case and the cost
// left is GetElement/SetElement on Int32Array/Uint8Array/Float64Array itself
// (gaussian-blur, stanford-crypto, gbemu memory-array shape).
// Measured (buildbox3, 2026-10-10, jsse HEAD 2b621c5 / boa 0.22.0, median of 5): jsse 2.22 s, boa 1.27 s, ratio 1.75x, jsse --no-bytecode 4.64 s.
function work(heap32, heap8, f64, n) {
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
console.log("typedarray_local", work(new Int32Array(1 << 14), new Uint8Array(1 << 14), new Float64Array(256), 400000));
