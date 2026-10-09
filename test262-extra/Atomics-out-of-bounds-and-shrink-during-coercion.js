/*---
description: >
  Atomics operations throw a TypeError for an out-of-bounds typed array and
  revalidate the access after argument coercion, throwing a RangeError if the
  buffer shrank below the byte index.
esid: sec-revalidateatomicaccess
info: |
  ValidateIntegerTypedArray calls ValidateTypedArray, which throws a TypeError
  when the typed array is out of bounds. RevalidateAtomicAccess runs after all
  argument coercions and throws a RangeError when byteIndexInBuffer is not less
  than the buffer's byte length.
features: [Atomics, resizable-arraybuffer, TypedArray]
---*/

function makeOutOfBounds() {
  var rab = new ArrayBuffer(16, { maxByteLength: 32 });
  var ta = new Int32Array(rab, 8);
  rab.resize(4);
  return ta;
}

assert.throws(TypeError, function() { Atomics.load(makeOutOfBounds(), 0); }, "load");
assert.throws(TypeError, function() { Atomics.store(makeOutOfBounds(), 0, 1); }, "store");
assert.throws(TypeError, function() { Atomics.add(makeOutOfBounds(), 0, 1); }, "add");
assert.throws(TypeError, function() { Atomics.compareExchange(makeOutOfBounds(), 0, 0, 1); }, "compareExchange");

var fixedRab = new ArrayBuffer(16, { maxByteLength: 32 });
var fixed = new Int32Array(fixedRab, 8, 2);
fixedRab.resize(8);
assert.throws(TypeError, function() { Atomics.load(fixed, 0); }, "fixed-length view past the buffer end");

function shrinker(rab, newLength, value) {
  return { valueOf: function() { rab.resize(newLength); return value; } };
}

var rab = new ArrayBuffer(16, { maxByteLength: 32 });
var view = new Int32Array(rab);
assert.throws(RangeError, function() {
  Atomics.load(view, shrinker(rab, 4, 3));
}, "load: index coercion shrinks the buffer");

rab = new ArrayBuffer(16, { maxByteLength: 32 });
view = new Int32Array(rab);
assert.throws(RangeError, function() {
  Atomics.store(view, 3, shrinker(rab, 4, 5));
}, "store: value coercion shrinks the buffer");

rab = new ArrayBuffer(16, { maxByteLength: 32 });
view = new Int32Array(rab);
assert.throws(RangeError, function() {
  Atomics.add(view, 3, shrinker(rab, 4, 5));
}, "add: value coercion shrinks the buffer");

rab = new ArrayBuffer(16, { maxByteLength: 32 });
view = new Int32Array(rab);
assert.throws(RangeError, function() {
  Atomics.compareExchange(view, 3, shrinker(rab, 4, 0), 1);
}, "compareExchange: expected-value coercion shrinks the buffer");
