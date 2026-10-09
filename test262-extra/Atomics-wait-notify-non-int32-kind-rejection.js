/*---
description: >
  Atomics.wait and Atomics.waitAsync only accept Int32Array and BigInt64Array,
  rejecting every other integer typed array kind, and reject Number/BigInt
  mismatches only through the usual value conversion.
esid: sec-dowait
info: |
  DoWait step 1 calls ValidateIntegerTypedArray(typedArray, true), which throws
  a TypeError unless the element type is Int32 or BigInt64.
features: [Atomics, Atomics.waitAsync, SharedArrayBuffer, TypedArray, BigInt]
---*/

var otherKinds = [Int8Array, Uint8Array, Int16Array, Uint16Array, Uint32Array, BigUint64Array];

otherKinds.forEach(function(TA) {
  var ta = new TA(new SharedArrayBuffer(16));
  var value = TA === BigUint64Array ? 0n : 0;
  assert.throws(TypeError, function() {
    Atomics.wait(ta, 0, value, 0);
  }, "Atomics.wait rejects " + TA.name);
  assert.throws(TypeError, function() {
    Atomics.waitAsync(ta, 0, value, 0);
  }, "Atomics.waitAsync rejects " + TA.name);
});

var big = new BigInt64Array(new SharedArrayBuffer(16));
assert.throws(TypeError, function() {
  Atomics.waitAsync(big, 0, 0, 0);
}, "Number value is not converted for a BigInt64Array");
assert.sameValue(Atomics.waitAsync(big, 0, 1n, 0).value, "not-equal", "BigInt64Array accepts BigInt values");
assert.sameValue(Atomics.waitAsync(big, 0, 0n, 0).value, "timed-out", "zero timeout times out synchronously");
