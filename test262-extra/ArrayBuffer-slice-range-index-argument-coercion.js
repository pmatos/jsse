/*---
description: >
  ArrayBuffer.prototype.slice, SharedArrayBuffer.prototype.slice and
  ArrayBuffer.prototype.sliceToImmutable resolve their start/end index arguments
  the same way Array.prototype.slice does: an absent start defaults to 0, an
  absent OR explicitly `undefined` end defaults to the byte length, every present
  argument is coerced through ToIntegerOrInfinity — truncating toward zero BEFORE
  the relative offset is computed — and the relative index is clamped against the
  length. A negative *fractional* argument such as -0.9 must therefore truncate to
  0 (a full-length copy), not fall through to `len + (-0.9)`. This mirrors the
  Array / TypedArray / String range-index coercion tests; the engine factors all
  three ArrayBuffer slice surfaces through the shared resolve_start_index /
  resolve_end_index helpers.
esid: sec-arraybuffer.prototype.slice
info: |
  ArrayBuffer.prototype.slice ( start, end )
    5. Let len be O.[[ArrayBufferByteLength]].
    6. Let relativeStart be ? ToIntegerOrInfinity(start).
    7. If relativeStart is -infinity, let first be 0.
       Else if relativeStart < 0, let first be max(len + relativeStart, 0).
       Else, let first be min(relativeStart, len).
    8. If end is undefined, let relativeEnd be len;
       else let relativeEnd be ? ToIntegerOrInfinity(end).
    9. If relativeEnd is -infinity, let final be 0.
       Else if relativeEnd < 0, let final be max(len + relativeEnd, 0).
       Else, let final be min(relativeEnd, len).
   10. Let newLen be max(final - first, 0).
  ToIntegerOrInfinity ( argument )
    Let number be ? ToNumber(argument).
    ... return truncate(number).
includes: [compareArray.js]
features: [SharedArrayBuffer, immutable-arraybuffer]
---*/

// bytesOf(buffer) → the buffer's bytes as a plain Array. Works for ArrayBuffer,
// SharedArrayBuffer and immutable ArrayBuffer alike (a Uint8Array view reads any
// of them).
function bytesOf(buffer) {
  return Array.from(new Uint8Array(buffer));
}

// filled(len) → a fresh ArrayBuffer of `len` bytes holding 0, 1, 2, ... len-1,
// so a slice's byte content pins the resolved [first, final) offsets — not just
// the resulting byteLength.
function filled(len) {
  var buf = new ArrayBuffer(len);
  var view = new Uint8Array(buf);
  for (var i = 0; i < len; i++) view[i] = i;
  return buf;
}

// ---------------------------------------------------------------------------
// ArrayBuffer.prototype.slice
// ---------------------------------------------------------------------------

// Absent / undefined defaults.
assert.compareArray(bytesOf(filled(5).slice()), [0, 1, 2, 3, 4], "slice(): whole buffer");
assert.compareArray(bytesOf(filled(5).slice(1)), [1, 2, 3, 4], "slice(1): absent end is length");
assert.compareArray(bytesOf(filled(5).slice(1, undefined)), [1, 2, 3, 4], "slice(1, undefined): undefined end is length");

// Negative arguments count back from the length.
assert.compareArray(bytesOf(filled(5).slice(-2)), [3, 4], "slice(-2)");
assert.compareArray(bytesOf(filled(5).slice(1, -1)), [1, 2, 3], "slice(1, -1)");

// Out-of-range clamps into [0, len].
assert.compareArray(bytesOf(filled(5).slice(-100, 100)), [0, 1, 2, 3, 4], "slice(-100, 100): clamps to [0, len]");

// Negative *fractional* arguments truncate toward zero FIRST, then take the
// relative offset. -0.9 truncates to -0 → 0 (a full copy), NOT len + (-0.9).
assert.sameValue(filled(10).slice(-0.9).byteLength, 10, "slice(-0.9).byteLength: -0.9 truncates to 0");
assert.compareArray(bytesOf(filled(10).slice(-0.9)), [0, 1, 2, 3, 4, 5, 6, 7, 8, 9], "slice(-0.9): full copy, not a 1-byte tail");
assert.compareArray(bytesOf(filled(10).slice(-0.9, -0.9)), [], "slice(-0.9, -0.9): empty, both ends truncate to 0");

// A negative fractional pair that resolves to real offsets: -3.9 → 7, -1.9 → 9.
// The byteLength alone (2) does not catch a truncation bug — the byte *content*
// does: an untruncated resolve would read [6, 7] instead of [7, 8].
assert.compareArray(bytesOf(filled(10).slice(-3.9, -1.9)), [7, 8], "slice(-3.9, -1.9): offsets 7..9, not 6..8");

// Positive fractional truncates toward zero: 2.9 → 2.
assert.compareArray(bytesOf(filled(10).slice(2.9)), [2, 3, 4, 5, 6, 7, 8, 9], "slice(2.9): start truncates to 2");

// NaN → 0; -Infinity → 0; +Infinity → len.
assert.compareArray(bytesOf(filled(4).slice(NaN)), [0, 1, 2, 3], "slice(NaN): NaN is 0");
assert.compareArray(bytesOf(filled(4).slice(-Infinity)), [0, 1, 2, 3], "slice(-Infinity): -Infinity is 0");
assert.compareArray(bytesOf(filled(4).slice(Infinity)), [], "slice(Infinity): +Infinity is len");

// ---------------------------------------------------------------------------
// SharedArrayBuffer.prototype.slice — the same resolution
// ---------------------------------------------------------------------------

// filledShared(len) → a SharedArrayBuffer of `len` bytes holding 0, 1, ... len-1.
function filledShared(len) {
  var sab = new SharedArrayBuffer(len);
  var view = new Uint8Array(sab);
  for (var i = 0; i < len; i++) view[i] = i;
  return sab;
}

assert.compareArray(bytesOf(filledShared(5).slice(1)), [1, 2, 3, 4], "SAB slice(1): absent end is length");
assert.compareArray(bytesOf(filledShared(5).slice(1, undefined)), [1, 2, 3, 4], "SAB slice(1, undefined): undefined end is length");
assert.compareArray(bytesOf(filledShared(5).slice(1, -1)), [1, 2, 3], "SAB slice(1, -1)");

// Negative fractional truncates toward zero before the relative offset.
assert.sameValue(filledShared(10).slice(-0.9).byteLength, 10, "SAB slice(-0.9).byteLength: -0.9 truncates to 0");
assert.compareArray(bytesOf(filledShared(10).slice(-0.9)), [0, 1, 2, 3, 4, 5, 6, 7, 8, 9], "SAB slice(-0.9): full copy");
assert.compareArray(bytesOf(filledShared(10).slice(-3.9, -1.9)), [7, 8], "SAB slice(-3.9, -1.9): offsets 7..9, not 6..8");
assert.compareArray(bytesOf(filledShared(10).slice(2.9)), [2, 3, 4, 5, 6, 7, 8, 9], "SAB slice(2.9): start truncates to 2");
assert.compareArray(bytesOf(filledShared(4).slice(NaN)), [0, 1, 2, 3], "SAB slice(NaN): NaN is 0");

// ---------------------------------------------------------------------------
// ArrayBuffer.prototype.sliceToImmutable — the same resolution
// ---------------------------------------------------------------------------

if (typeof ArrayBuffer.prototype.sliceToImmutable === "function") {
  assert.compareArray(bytesOf(filled(5).sliceToImmutable(1)), [1, 2, 3, 4], "sliceToImmutable(1): absent end is length");
  assert.compareArray(bytesOf(filled(5).sliceToImmutable(1, undefined)), [1, 2, 3, 4], "sliceToImmutable(1, undefined): undefined end is length");
  assert.compareArray(bytesOf(filled(5).sliceToImmutable(1, -1)), [1, 2, 3], "sliceToImmutable(1, -1)");
  assert.compareArray(bytesOf(filled(10).sliceToImmutable(-0.9)), [0, 1, 2, 3, 4, 5, 6, 7, 8, 9], "sliceToImmutable(-0.9): full copy");
  assert.compareArray(bytesOf(filled(10).sliceToImmutable(-3.9, -1.9)), [7, 8], "sliceToImmutable(-3.9, -1.9): offsets 7..9");
  assert.compareArray(bytesOf(filled(10).sliceToImmutable(2.9)), [2, 3, 4, 5, 6, 7, 8, 9], "sliceToImmutable(2.9): start truncates to 2");
  assert.compareArray(bytesOf(filled(4).sliceToImmutable(NaN)), [0, 1, 2, 3], "sliceToImmutable(NaN): NaN is 0");
}

// ---------------------------------------------------------------------------
// Coercion order: start is coerced before end, and a throwing start argument
// short-circuits before end is touched. All three surfaces share this order.
// ---------------------------------------------------------------------------

function orderLog(makeBuffer, method) {
  var log = [];
  var start = { valueOf: function() { log.push("start"); return 1; } };
  var end = { valueOf: function() { log.push("end"); return 3; } };
  makeBuffer()[method](start, end);
  return log.join(",");
}

assert.sameValue(orderLog(function() { return new ArrayBuffer(5); }, "slice"), "start,end", "ArrayBuffer.slice coerces start then end");
assert.sameValue(orderLog(function() { return new SharedArrayBuffer(5); }, "slice"), "start,end", "SharedArrayBuffer.slice coerces start then end");
if (typeof ArrayBuffer.prototype.sliceToImmutable === "function") {
  assert.sameValue(orderLog(function() { return new ArrayBuffer(5); }, "sliceToImmutable"), "start,end", "sliceToImmutable coerces start then end");
}

function throwsBeforeEnd(makeBuffer, method) {
  var endTouched = false;
  var start = { valueOf: function() { throw new Test262Error("start"); } };
  var end = { valueOf: function() { endTouched = true; return 0; } };
  assert.throws(Test262Error, function() { makeBuffer()[method](start, end); }, method + ": throwing start propagates");
  return endTouched;
}

assert.sameValue(throwsBeforeEnd(function() { return new ArrayBuffer(5); }, "slice"), false, "ArrayBuffer.slice: throwing start never coerces end");
assert.sameValue(throwsBeforeEnd(function() { return new SharedArrayBuffer(5); }, "slice"), false, "SharedArrayBuffer.slice: throwing start never coerces end");
