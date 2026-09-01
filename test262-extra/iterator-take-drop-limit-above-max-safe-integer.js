/*---
description: >
  Iterator.prototype.take and Iterator.prototype.drop throw a RangeError when
  the limit is finite and greater than 2**53 - 1, closing the underlying
  iterator, while accepting 2**53 - 1 itself and Infinity.
esid: sec-iterator.prototype.take
info: |
  Iterator.prototype.take ( limit )

  1. Let O be the this value.
  2. If O is not an Object, throw a TypeError exception.
  3. Let numLimit be ? ToNumber(limit).
  4. If numLimit is NaN, then
     a. Let error be ThrowCompletion(a newly created RangeError object).
     b. Return ? IteratorClose(iterated, error).
  5. If numLimit is finite and numLimit > 𝔽(2**53 - 1), then
     a. Let error be ThrowCompletion(a newly created RangeError object).
     b. Return ? IteratorClose(iterated, error).
  6. Let integerLimit be ! ToIntegerOrInfinity(numLimit).
  7. If integerLimit < 0, then
     a. Let error be ThrowCompletion(a newly created RangeError object).
     b. Return ? IteratorClose(iterated, error).

  Iterator.prototype.drop ( limit ) performs the same steps.

  Step 5 distinguishes a finite limit above the safe-integer range, which is
  rejected, from Infinity, which is not finite and so is accepted.
features: [iterator-helpers]
---*/

var MAX_SAFE = 9007199254740991; // 2**53 - 1

// Values at or below 2**53 - 1, and Infinity, are accepted.
(function* () {})().take(MAX_SAFE);
(function* () {})().drop(MAX_SAFE);
(function* () {})().take(Infinity);
(function* () {})().drop(Infinity);

// A finite limit above 2**53 - 1 is a RangeError.
assert.throws(RangeError, function () {
  (function* () {})().take(MAX_SAFE + 1);
}, 'take with a limit above 2**53 - 1');

assert.throws(RangeError, function () {
  (function* () {})().drop(MAX_SAFE + 1);
}, 'drop with a limit above 2**53 - 1');

assert.throws(RangeError, function () {
  (function* () {})().take(Number.MAX_VALUE);
}, 'take with Number.MAX_VALUE');

assert.throws(RangeError, function () {
  (function* () {})().drop(Number.MAX_VALUE);
}, 'drop with Number.MAX_VALUE');

// Step 5.b closes the underlying iterator before the RangeError propagates.
function closingIterator() {
  var iterator = {
    returnCount: 0,
    next: function () {
      return { value: 1, done: false };
    },
    return: function () {
      this.returnCount += 1;
      return { done: true };
    }
  };
  Object.setPrototypeOf(iterator, Iterator.prototype);
  return iterator;
}

var takeIterator = closingIterator();
assert.throws(RangeError, function () {
  takeIterator.take(MAX_SAFE + 1);
});
assert.sameValue(takeIterator.returnCount, 1, 'take closed the underlying iterator');

var dropIterator = closingIterator();
assert.throws(RangeError, function () {
  dropIterator.drop(MAX_SAFE + 1);
});
assert.sameValue(dropIterator.returnCount, 1, 'drop closed the underlying iterator');

// The limit is coerced with ToNumber before the range check, so a valueOf that
// returns an out-of-range value still produces a RangeError.
var coerced = {
  valueOf: function () {
    return MAX_SAFE + 1;
  }
};

assert.throws(RangeError, function () {
  (function* () {})().take(coerced);
}, 'take with a coerced out-of-range limit');

assert.throws(RangeError, function () {
  (function* () {})().drop(coerced);
}, 'drop with a coerced out-of-range limit');
