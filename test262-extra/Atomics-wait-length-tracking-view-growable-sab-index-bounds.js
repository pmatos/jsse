/*---
description: >
  Atomics.wait and Atomics.waitAsync validate the index against the current
  length of a length-tracking view over a growable SharedArrayBuffer.
esid: sec-dowait
info: |
  DoWait calls ValidateAtomicAccessOnIntegerTypedArray, which uses
  TypedArrayLength(taRecord) of the current buffer size.
flags: [async]
features: [Atomics, Atomics.waitAsync, SharedArrayBuffer, resizable-arraybuffer, TypedArray]
---*/

var gsab = new SharedArrayBuffer(8, { maxByteLength: 32 });
var view = new Int32Array(gsab);

assert.throws(RangeError, function() {
  Atomics.waitAsync(view, 3, 0, 0);
}, "waitAsync index 3 is out of range before grow");

gsab.grow(16);

var immediate = Atomics.waitAsync(view, 3, 1, 0);
assert.sameValue(immediate.async, false, "value mismatch is synchronous");
assert.sameValue(immediate.value, "not-equal", "newly valid index is readable");

var pending = Atomics.waitAsync(view, 3, 0, 10000);
assert.sameValue(pending.async, true, "matching value waits asynchronously");
assert.sameValue(Atomics.notify(view, 3), 1, "notify wakes the waiter on the grown index");

pending.value.then(function(result) {
  assert.sameValue(result, "ok");
}).then($DONE, $DONE);
