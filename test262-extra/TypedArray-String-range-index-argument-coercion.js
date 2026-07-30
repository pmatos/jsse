/*---
description: >
  The range-based %TypedArray%.prototype methods (slice, subarray, copyWithin,
  fill) and String.prototype.slice resolve their start/end index arguments the
  same way Array.prototype.slice does: an absent start defaults to 0, an absent
  OR explicitly `undefined` end defaults to the length, every present argument is
  coerced through ToIntegerOrInfinity and the relative index is clamped against
  the length, and a throwing coercion is propagated unchanged. This mirrors the
  Array-range-index-argument-coercion test for the TypedArray and String
  surfaces, which the engine factors through the shared resolve_start_index /
  resolve_end_index helpers.
esid: sec-%typedarray%.prototype.slice
info: |
  %TypedArray%.prototype.slice ( start, end )
    ... Let relativeStart be ? ToIntegerOrInfinity(start).
        If relativeStart is -infinity, let k be 0.
        Else if relativeStart < 0, let k be max(len + relativeStart, 0).
        Else, let k be min(relativeStart, len).
    ... If end is undefined, let relativeEnd be len; else let relativeEnd be
        ? ToIntegerOrInfinity(end). (same clamping as start)
includes: [compareArray.js]
---*/

function i8(values) {
  return Array.from(new Int8Array(values));
}

// ---------------------------------------------------------------------------
// TypedArray.prototype.slice: start default 0, end default (absent/undefined) len
// ---------------------------------------------------------------------------
assert.compareArray(i8(new Int8Array([1, 2, 3, 4, 5]).slice()), [1, 2, 3, 4, 5], "slice(): whole array");
assert.compareArray(i8(new Int8Array([1, 2, 3, 4, 5]).slice(1)), [2, 3, 4, 5], "slice(1)");
assert.compareArray(i8(new Int8Array([1, 2, 3, 4, 5]).slice(-2)), [4, 5], "slice(-2)");
assert.compareArray(i8(new Int8Array([1, 2, 3, 4, 5]).slice(1, -1)), [2, 3, 4], "slice(1, -1)");
assert.compareArray(i8(new Int8Array([1, 2, 3, 4, 5]).slice(1, undefined)), [2, 3, 4, 5], "slice(1, undefined): undefined end is length");
assert.compareArray(i8(new Int8Array([1, 2, 3, 4, 5]).slice(-100, 100)), [1, 2, 3, 4, 5], "slice clamps out-of-range to [0, len]");

// ---------------------------------------------------------------------------
// TypedArray.prototype.subarray: same start/end resolution
// ---------------------------------------------------------------------------
assert.compareArray(i8(new Int8Array([1, 2, 3, 4, 5]).subarray(1)), [2, 3, 4, 5], "subarray(1)");
assert.compareArray(i8(new Int8Array([1, 2, 3, 4, 5]).subarray(-2)), [4, 5], "subarray(-2)");
assert.compareArray(i8(new Int8Array([1, 2, 3, 4, 5]).subarray(1, -1)), [2, 3, 4], "subarray(1, -1)");
assert.compareArray(i8(new Int8Array([1, 2, 3, 4, 5]).subarray(1, undefined)), [2, 3, 4, 5], "subarray(1, undefined): undefined end is length");

// ---------------------------------------------------------------------------
// TypedArray.prototype.copyWithin: target and start are `start`-style, end `end`-style
// ---------------------------------------------------------------------------
assert.compareArray(i8(new Int8Array([1, 2, 3, 4, 5]).copyWithin(0, 3)), [4, 5, 3, 4, 5], "copyWithin(0, 3)");
assert.compareArray(i8(new Int8Array([1, 2, 3, 4, 5]).copyWithin(0, 3, undefined)), [4, 5, 3, 4, 5], "copyWithin end undefined is length");
assert.compareArray(i8(new Int8Array([1, 2, 3, 4, 5]).copyWithin(0, undefined)), [1, 2, 3, 4, 5], "copyWithin start undefined is 0");
assert.compareArray(i8(new Int8Array([1, 2, 3, 4, 5]).copyWithin(-2, 0)), [1, 2, 3, 1, 2], "copyWithin(-2, 0): negative target");

// ---------------------------------------------------------------------------
// TypedArray.prototype.fill: value coerced first, then start/end resolution
// ---------------------------------------------------------------------------
assert.compareArray(i8(new Int8Array([0, 0, 0, 0, 0]).fill(7)), [7, 7, 7, 7, 7], "fill(7): absent start/end");
assert.compareArray(i8(new Int8Array([0, 0, 0, 0, 0]).fill(7, undefined)), [7, 7, 7, 7, 7], "fill(v, undefined): undefined start is 0");
assert.compareArray(i8(new Int8Array([0, 0, 0, 0, 0]).fill(9, -2)), [0, 0, 0, 9, 9], "fill(v, -2)");
assert.compareArray(i8(new Int8Array([0, 0, 0, 0, 0]).fill(7, 1, undefined)), [0, 7, 7, 7, 7], "fill(v, 1, undefined): undefined end is length");
assert.compareArray(i8(new Int8Array([0, 0, 0, 0, 0]).fill(3, 1, -1)), [0, 3, 3, 3, 0], "fill(v, 1, -1)");

// ---------------------------------------------------------------------------
// String.prototype.slice: start default 0, end default (absent/undefined) len
// ---------------------------------------------------------------------------
assert.sameValue("abcde".slice(), "abcde", "String.slice(): whole string");
assert.sameValue("abcde".slice(undefined), "abcde", "String.slice(undefined): undefined start is 0");
assert.sameValue("abcde".slice(1), "bcde", "String.slice(1)");
assert.sameValue("abcde".slice(-2), "de", "String.slice(-2)");
assert.sameValue("abcde".slice(1, -1), "bcd", "String.slice(1, -1)");
assert.sameValue("abcde".slice(1, undefined), "bcde", "String.slice(1, undefined): undefined end is length");
assert.sameValue("abcde".slice(-100, 100), "abcde", "String.slice clamps out-of-range to [0, len]");

// ---------------------------------------------------------------------------
// a throwing coercion is propagated unchanged from every coerced position
// ---------------------------------------------------------------------------
var boom = { valueOf: function () { throw new Test262Error("coercion ran"); } };

assert.throws(Test262Error, function () { new Int8Array([1, 2, 3]).slice(boom); }, "TA slice start throw propagates");
assert.throws(Test262Error, function () { new Int8Array([1, 2, 3]).slice(0, boom); }, "TA slice end throw propagates");
assert.throws(Test262Error, function () { new Int8Array([1, 2, 3]).subarray(boom); }, "TA subarray start throw propagates");
assert.throws(Test262Error, function () { new Int8Array([1, 2, 3]).copyWithin(boom, 0); }, "TA copyWithin target throw propagates");
assert.throws(Test262Error, function () { new Int8Array([1, 2, 3]).copyWithin(0, boom); }, "TA copyWithin start throw propagates");
assert.throws(Test262Error, function () { new Int8Array([1, 2, 3]).fill(0, boom); }, "TA fill start throw propagates");
assert.throws(Test262Error, function () { new Int8Array([1, 2, 3]).fill(0, 0, boom); }, "TA fill end throw propagates");
assert.throws(Test262Error, function () { "abc".slice(boom); }, "String slice start throw propagates");
assert.throws(Test262Error, function () { "abc".slice(0, boom); }, "String slice end throw propagates");
