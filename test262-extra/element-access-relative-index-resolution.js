/*---
description: >
  The element-access methods Array.prototype.at, String.prototype.at,
  %TypedArray%.prototype.at, Array.prototype.with and %TypedArray%.prototype.with
  resolve a spec "relative index" argument consistently: the argument is coerced
  through ToIntegerOrInfinity, a negative result counts back from the length, and
  an index that lands outside [0, len) yields no element — `.at` returns
  `undefined`, `.with` throws a RangeError. This differs from the range-based
  methods (slice/fill/copyWithin), which clamp an out-of-range index into
  [0, len]. This test pins the out-of-range / +-Infinity / boundary behaviour and
  the ToIntegerOrInfinity coercion (including that a throwing coercion is
  propagated unchanged), which the engine factors through the
  resolve_element_index helper.
esid: sec-array.prototype.at
info: |
  Array.prototype.at ( index )
    3. Let relativeIndex be ? ToIntegerOrInfinity(index).
    4. If relativeIndex >= 0, let k be relativeIndex.
       Else, let k be len + relativeIndex.
    5. If k < 0 or k >= len, return undefined.

  Array.prototype.with ( index, value )
    3. Let relativeIndex be ? ToIntegerOrInfinity(index).
    4. If relativeIndex >= 0, let actualIndex be relativeIndex.
       Else, let actualIndex be len + relativeIndex.
    5. If actualIndex >= len or actualIndex < 0, throw a RangeError exception.
includes: [compareArray.js]
---*/

// ---------------------------------------------------------------------------
// .at — in-range indices (positive and from-the-end) return the element
// ---------------------------------------------------------------------------
assert.sameValue([10, 20, 30].at(0), 10, "Array.at(0)");
assert.sameValue([10, 20, 30].at(2), 30, "Array.at(2): last element");
assert.sameValue([10, 20, 30].at(-1), 30, "Array.at(-1): counts back from the end");
assert.sameValue([10, 20, 30].at(-3), 10, "Array.at(-len): first element");

assert.sameValue("abc".at(-1), "c", "String.at(-1)");
assert.sameValue("abc".at(0), "a", "String.at(0)");

var ta = new Int8Array([5, 6, 7]);
assert.sameValue(ta.at(-1), 7, "TypedArray.at(-1)");
assert.sameValue(ta.at(0), 5, "TypedArray.at(0)");

// ---------------------------------------------------------------------------
// .at — out-of-range indices return undefined (NOT clamped)
// ---------------------------------------------------------------------------
assert.sameValue([10, 20, 30].at(3), undefined, "Array.at(len) is out of range");
assert.sameValue([10, 20, 30].at(-4), undefined, "Array.at(-(len)-1) is out of range");
assert.sameValue([10, 20, 30].at(Infinity), undefined, "Array.at(Infinity) is out of range");
assert.sameValue([10, 20, 30].at(-Infinity), undefined, "Array.at(-Infinity) is out of range");
assert.sameValue([10, 20, 30].at(1e300), undefined, "Array.at(huge) is out of range");

assert.sameValue("abc".at(3), undefined, "String.at(len) is out of range");
assert.sameValue("abc".at(-4), undefined, "String.at(-(len)-1) is out of range");
assert.sameValue("abc".at(-Infinity), undefined, "String.at(-Infinity) is out of range");

assert.sameValue(ta.at(3), undefined, "TypedArray.at(len) is out of range");
assert.sameValue(ta.at(-4), undefined, "TypedArray.at(-(len)-1) is out of range");
assert.sameValue(ta.at(Infinity), undefined, "TypedArray.at(Infinity) is out of range");

// ---------------------------------------------------------------------------
// .at — ToIntegerOrInfinity coercion (NaN -> 0, truncation, -0 -> 0)
// ---------------------------------------------------------------------------
assert.sameValue([10, 20, 30].at(NaN), 10, "Array.at(NaN) coerces to 0");
assert.sameValue([10, 20, 30].at(undefined), 10, "Array.at(undefined) coerces to 0");
assert.sameValue([10, 20, 30].at(1.9), 20, "Array.at(1.9) truncates to 1");
assert.sameValue([10, 20, 30].at(-0), 10, "Array.at(-0) is index 0");
assert.sameValue([10, 20, 30].at("x"), 10, "Array.at(non-numeric string) coerces to 0");
assert.sameValue([10, 20, 30].at("2"), 30, "Array.at('2') coerces to 2");

// ---------------------------------------------------------------------------
// .with — in-range writes produce a new array with one element replaced
// ---------------------------------------------------------------------------
assert.compareArray([1, 2, 3].with(0, 9), [9, 2, 3], "Array.with(0, v)");
assert.compareArray([1, 2, 3].with(-1, 9), [1, 2, 9], "Array.with(-1, v) counts from the end");
assert.compareArray([1, 2, 3].with(-3, 9), [9, 2, 3], "Array.with(-len, v)");

assert.compareArray(
  Array.from(new Int8Array([1, 2, 3]).with(-1, 9)),
  [1, 2, 9],
  "TypedArray.with(-1, v)"
);

// ---------------------------------------------------------------------------
// .with — out-of-range indices throw RangeError (NOT clamped)
// ---------------------------------------------------------------------------
assert.throws(RangeError, function () { [1, 2, 3].with(3, 9); }, "Array.with(len) throws");
assert.throws(RangeError, function () { [1, 2, 3].with(-4, 9); }, "Array.with(-(len)-1) throws");
assert.throws(RangeError, function () { [1, 2, 3].with(Infinity, 9); }, "Array.with(Infinity) throws");
assert.throws(RangeError, function () { [1, 2, 3].with(-Infinity, 9); }, "Array.with(-Infinity) throws");
assert.throws(RangeError, function () { new Int8Array([1, 2, 3]).with(3, 9); }, "TypedArray.with(len) throws");
assert.throws(RangeError, function () { new Int8Array([1, 2, 3]).with(-4, 9); }, "TypedArray.with(-(len)-1) throws");

// ---------------------------------------------------------------------------
// a throwing index coercion is propagated unchanged
// ---------------------------------------------------------------------------
var boom = { valueOf: function () { throw new Test262Error("coercion ran"); } };

assert.throws(Test262Error, function () { [1, 2, 3].at(boom); }, "Array.at index coercion throw propagates");
assert.throws(Test262Error, function () { "abc".at(boom); }, "String.at index coercion throw propagates");
assert.throws(Test262Error, function () { new Int8Array([1, 2, 3]).at(boom); }, "TypedArray.at index coercion throw propagates");
assert.throws(Test262Error, function () { [1, 2, 3].with(boom, 0); }, "Array.with index coercion throw propagates");
