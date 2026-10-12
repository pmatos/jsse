// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-stringpad
description: >
  padStart, padEnd and repeat throw a RangeError, rather than aborting the
  engine, when the requested length exceeds the implementation's maximum string
  length, and keep returning the receiver or the empty string when no padding
  is generated.
---*/

// Tests issue #940. The ECMAScript String type has no length cap below 2^53 - 1
// code units, but an implementation must fail a request it cannot satisfy with
// a catchable error. Every call below fails (or succeeds trivially) before any
// large allocation, so the test stays within the runner's memory limit.
// Spec: ECMAScript 2024, sec-stringpad, sec-string.prototype.repeat

assert.throws(RangeError, function() { 'a'.padStart(Infinity); }, 'padStart Infinity');
assert.throws(RangeError, function() { 'a'.padEnd(Infinity); }, 'padEnd Infinity');
assert.throws(RangeError, function() { 'a'.padStart(Math.pow(2, 53) - 1); }, 'padStart 2**53-1');
assert.throws(RangeError, function() { 'a'.padEnd(Math.pow(2, 53) - 1); }, 'padEnd 2**53-1');
assert.throws(RangeError, function() { 'a'.padStart(Math.pow(2, 53) - 1, 'xy'); }, 'padStart 2**53-1 with fill');
assert.throws(RangeError, function() { 'a'.padEnd(Math.pow(2, 62), 'xy'); }, 'padEnd 2**62 with fill');

// 2^29 - 23 is one code unit past the maximum length.
assert.throws(RangeError, function() { ''.padStart(Math.pow(2, 29) - 23, 'x'); }, 'padStart max + 1');
assert.throws(RangeError, function() { ''.padEnd(Math.pow(2, 29) - 23, 'x'); }, 'padEnd max + 1');

assert.throws(RangeError, function() { 'ab'.repeat(Math.pow(2, 60)); }, 'repeat 2**60');
assert.throws(RangeError, function() { 'ab'.repeat(Math.pow(2, 53) - 1); }, 'repeat 2**53-1');
assert.throws(RangeError, function() { 'a'.repeat(Math.pow(2, 29) - 23); }, 'repeat max + 1');
assert.throws(RangeError, function() { 'ab'.repeat(Math.pow(2, 28) - 11); }, 'repeat 2 * count past max');
assert.throws(RangeError, function() { 'abc'.repeat(Math.pow(2, 32)); }, 'repeat 3 * 2**32');

// The thrown error comes from the receiving realm's RangeError constructor.
try {
  'a'.repeat(Math.pow(2, 60));
  throw new Test262Error('repeat 2**60 did not throw');
} catch (e) {
  assert.sameValue(e.constructor, RangeError, 'error constructor');
}

// Nothing is generated, so no limit applies.
assert.sameValue(''.repeat(Math.pow(2, 60)), '', 'empty receiver repeats to empty');
assert.sameValue(''.repeat(Math.pow(2, 53) - 1), '', 'empty receiver, 2**53-1');
assert.sameValue('abc'.repeat(0), '', 'count 0');
assert.sameValue('abc'.padStart(Infinity, ''), 'abc', 'padStart with empty fill');
assert.sameValue('abc'.padEnd(Math.pow(2, 60), ''), 'abc', 'padEnd with empty fill');
assert.sameValue('abc'.padStart(2), 'abc', 'padStart shorter than receiver');
assert.sameValue('abc'.padEnd(-Infinity, 'x'), 'abc', 'padEnd -Infinity');
assert.sameValue('abc'.padStart(NaN, 'x'), 'abc', 'padStart NaN');

// Invalid counts keep reporting a RangeError.
assert.throws(RangeError, function() { 'a'.repeat(-1); }, 'repeat -1');
assert.throws(RangeError, function() { 'a'.repeat(-Infinity); }, 'repeat -Infinity');

// The fill string is coerced (step 5) before the length is rejected.
var calls = 0;
var fill = { toString: function() { calls++; return 'x'; } };
assert.throws(RangeError, function() { 'a'.padStart(Math.pow(2, 60), fill); }, 'padStart with object fill');
assert.sameValue(calls, 1, 'fill ToString called once before the RangeError');

// The count is coerced before the length is rejected.
calls = 0;
var count = { valueOf: function() { calls++; return Math.pow(2, 60); } };
assert.throws(RangeError, function() { 'a'.repeat(count); }, 'repeat with object count');
assert.sameValue(calls, 1, 'count valueOf called once');

// A result that fits is unaffected.
assert.sameValue('ab'.repeat(3), 'ababab', 'small repeat');
assert.sameValue('ab'.padStart(7, 'xyz'), 'xyzxyab', 'small padStart');
assert.sameValue('ab'.padEnd(7, 'xyz'), 'abxyzxy', 'small padEnd');
