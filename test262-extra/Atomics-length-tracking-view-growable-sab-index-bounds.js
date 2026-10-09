/*---
description: >
  Atomics operations validate the access index against the current length of a
  length-tracking typed array, so indices that become valid after a growable
  SharedArrayBuffer grows are accepted.
esid: sec-validateatomicaccess
info: |
  ValidateAtomicAccess ( taRecord, requestIndex )
    ...
    3. Let length be TypedArrayLength(taRecord).
    ...
    5. If accessIndex ≥ length, throw a RangeError exception.

  TypedArrayLength recomputes the length from the buffer's current byte length
  when the view is length-tracking. JSSE validated against the length cached at
  construction time, rejecting indices made valid by SharedArrayBuffer.prototype.grow.
includes: [compareArray.js]
features: [Atomics, Atomics.waitAsync, SharedArrayBuffer, resizable-arraybuffer, BigInt, TypedArray]
---*/

var gsab = new SharedArrayBuffer(8, { maxByteLength: 32 });
var tracking = new Int32Array(gsab);

assert.throws(RangeError, function() {
  Atomics.store(tracking, 3, 1);
}, "index 3 is out of range before grow");

gsab.grow(16);
assert.sameValue(tracking.length, 4, "view length tracks the grown buffer");

assert.sameValue(Atomics.store(tracking, 3, 9), 9, "store at newly valid index");
assert.sameValue(Atomics.load(tracking, 3), 9, "load at newly valid index");
assert.sameValue(Atomics.add(tracking, 3, 1), 9, "add returns old value");
assert.sameValue(Atomics.exchange(tracking, 3, 2), 10, "exchange returns old value");
assert.sameValue(Atomics.compareExchange(tracking, 3, 2, 5), 2, "compareExchange returns old value");
assert.sameValue(new Int32Array(gsab)[3], 5, "a fresh view observes the write");

assert.sameValue(Atomics.notify(tracking, 3), 0, "notify accepts the newly valid index");
assert.throws(RangeError, function() {
  Atomics.notify(tracking, 4);
}, "index 4 is still out of range");

var offsetGsab = new SharedArrayBuffer(8, { maxByteLength: 32 });
var offsetView = new Int32Array(offsetGsab, 4);
assert.sameValue(offsetView.length, 1);
offsetGsab.grow(24);
assert.sameValue(offsetView.length, 5, "offset view length accounts for byteOffset");
Atomics.store(offsetView, 4, 7);
assert.sameValue(Atomics.load(offsetView, 4), 7, "last element of the grown offset view");
assert.throws(RangeError, function() {
  Atomics.load(offsetView, 5);
}, "one past the grown length");

var bigGsab = new SharedArrayBuffer(8, { maxByteLength: 32 });
var bigView = new BigInt64Array(bigGsab);
bigGsab.grow(24);
assert.sameValue(Atomics.store(bigView, 2, 7n), 7n, "BigInt store at newly valid index");
assert.sameValue(Atomics.load(bigView, 2), 7n, "BigInt load at newly valid index");

var fixedGsab = new SharedArrayBuffer(8, { maxByteLength: 32 });
var fixedView = new Int32Array(fixedGsab, 0, 2);
fixedGsab.grow(32);
assert.sameValue(fixedView.length, 2, "explicit-length view does not track growth");
assert.throws(RangeError, function() {
  Atomics.add(fixedView, 2, 1);
}, "explicit-length view still rejects index 2");
