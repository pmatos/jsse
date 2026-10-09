/*---
description: >
  Every Atomics operation rejects non-integer typed array kinds with a
  TypeError, for shared and non-shared buffers alike.
esid: sec-validateintegertypedarray
info: |
  ValidateIntegerTypedArray ( typedArray, waitable )
    ...
    5. Else,
      a. Let type be TypedArrayElementType(typedArray).
      b. If IsUnclampedIntegerElementType(type) is false and IsBigIntElementType(type) is false, throw a TypeError exception.
features: [Atomics, Atomics.waitAsync, SharedArrayBuffer, TypedArray, Float16Array, Float32Array, Float64Array]
---*/

var ctors = [Float16Array, Float32Array, Float64Array, Uint8ClampedArray];
var buffers = [ArrayBuffer, SharedArrayBuffer];
var ops = ["load", "store", "add", "sub", "and", "or", "xor", "exchange",
           "compareExchange", "wait", "notify", "waitAsync"];

ctors.forEach(function(TA) {
  buffers.forEach(function(Buf) {
    ops.forEach(function(op) {
      var ta = new TA(new Buf(16));
      assert.throws(TypeError, function() {
        Atomics[op](ta, 0, 0, 0);
      }, TA.name + " over " + Buf.name + " rejected by Atomics." + op);
    });
  });
});
