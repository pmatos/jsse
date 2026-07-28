/*---
description: >
  The range-based Array.prototype methods (slice, fill, copyWithin, splice,
  toSpliced) resolve their start/end index arguments consistently: an absent
  start defaults to 0, an absent OR explicitly `undefined` end defaults to the
  length, every present argument is coerced through ToIntegerOrInfinity and the
  relative index is clamped against the length, and a throwing coercion is
  propagated unchanged.
esid: sec-array.prototype.slice
info: |
  Array.prototype.slice ( start, end )
    3. Let relativeStart be ? ToIntegerOrInfinity(start).
    4. If relativeStart is -infinity, let k be 0.
       Else if relativeStart < 0, let k be max(len + relativeStart, 0).
       Else, let k be min(relativeStart, len).
    5. If end is undefined, let relativeEnd be len;
       else let relativeEnd be ? ToIntegerOrInfinity(end).
    6. If relativeEnd is -infinity, let final be 0.
       Else if relativeEnd < 0, let final be max(len + relativeEnd, 0).
       Else, let final be min(relativeEnd, len).

  The `start`-style argument (slice/fill/copyWithin/splice/toSpliced start,
  copyWithin target) does NOT special-case `undefined`: ToIntegerOrInfinity(
  undefined) is NaN, which becomes 0. Only the `end`-style argument treats
  `undefined` as "use length". This test pins that distinction and the shared
  clamping/coercion, which the engine factors through the resolve_start_index /
  resolve_end_index helpers.
includes: [compareArray.js]
---*/

// --- start argument: absent and `undefined` both resolve to 0 ---
assert.compareArray([10, 20, 30, 40, 50].slice(), [10, 20, 30, 40, 50], "slice(): absent start is 0");
assert.compareArray([10, 20, 30, 40, 50].slice(undefined), [10, 20, 30, 40, 50], "slice(undefined): undefined start is 0, not length");
assert.compareArray([0, 0, 0, 0, 0].fill(7, undefined), [7, 7, 7, 7, 7], "fill(v, undefined): undefined start is 0");

// --- end argument: absent and `undefined` both resolve to length ---
assert.compareArray([10, 20, 30, 40, 50].slice(1), [20, 30, 40, 50], "slice(1): absent end is length");
assert.compareArray([10, 20, 30, 40, 50].slice(1, undefined), [20, 30, 40, 50], "slice(1, undefined): undefined end is length");
assert.compareArray([0, 0, 0, 0, 0].fill(7, 1, undefined), [0, 7, 7, 7, 7], "fill(v, 1, undefined): undefined end is length");

// --- negative indices count back from the length; overshoot clamps ---
assert.compareArray([10, 20, 30, 40, 50].slice(-2), [40, 50], "slice(-2)");
assert.compareArray([10, 20, 30, 40, 50].slice(1, -1), [20, 30, 40], "slice(1, -1)");
assert.compareArray([10, 20, 30, 40, 50].slice(-100, 100), [10, 20, 30, 40, 50], "slice clamps out-of-range to [0, len]");
assert.compareArray([0, 0, 0, 0, 0].fill(9, -2), [0, 0, 0, 9, 9], "fill(v, -2)");

// --- copyWithin: target and start are `start`-style, end is `end`-style ---
assert.compareArray([1, 2, 3, 4, 5].copyWithin(0, 3, undefined), [4, 5, 3, 4, 5], "copyWithin end undefined is length");
assert.compareArray([1, 2, 3, 4, 5].copyWithin(0, undefined), [1, 2, 3, 4, 5], "copyWithin start undefined is 0");

// --- splice: a single `undefined` start removes from index 0 to the end ---
var spliced = [10, 20, 30, 40, 50];
var removed = spliced.splice(undefined);
assert.compareArray(removed, [10, 20, 30, 40, 50], "splice(undefined) removes from index 0");
assert.compareArray(spliced, [], "splice(undefined) empties the array");

// --- toSpliced: `undefined` start with an explicit delete count of 0 ---
assert.compareArray([10, 20, 30].toSpliced(undefined, 0, 99), [99, 10, 20, 30], "toSpliced(undefined, 0, 99) inserts at index 0");

// --- a throwing coercion is propagated unchanged from every coerced position ---
var boom = { valueOf: function () { throw new Test262Error("coercion ran"); } };

assert.throws(Test262Error, function () { [1, 2, 3].slice(boom); }, "slice start coercion throw propagates");
assert.throws(Test262Error, function () { [1, 2, 3].slice(0, boom); }, "slice end coercion throw propagates");
assert.throws(Test262Error, function () { [1, 2, 3].fill(0, boom); }, "fill start coercion throw propagates");
assert.throws(Test262Error, function () { [1, 2, 3].fill(0, 0, boom); }, "fill end coercion throw propagates");
assert.throws(Test262Error, function () { [1, 2, 3].copyWithin(boom, 0); }, "copyWithin target coercion throw propagates");
assert.throws(Test262Error, function () { [1, 2, 3].copyWithin(0, boom); }, "copyWithin start coercion throw propagates");
assert.throws(Test262Error, function () { [1, 2, 3].copyWithin(0, 0, boom); }, "copyWithin end coercion throw propagates");
assert.throws(Test262Error, function () { [1, 2, 3].splice(boom); }, "splice start coercion throw propagates");
