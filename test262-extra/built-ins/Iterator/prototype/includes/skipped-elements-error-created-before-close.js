/*---
description: >
  Iterator.prototype.includes creates the skippedElements validation error
  before closing the underlying iterator, so a return() method that replaces
  the global error constructor cannot change the thrown error's prototype.
esid: sec-iterator.prototype.includes
info: |
  Iterator.prototype.includes ( searchElement [ , skippedElements ] )

  6. If toSkip is not an integral Number, then
    a. Let error be ThrowCompletion(a newly created TypeError object).
    b. Return ? IteratorClose(iterated, error).
  7. If toSkip < -01, then
    a. Let error be ThrowCompletion(a newly created RangeError object).
    b. Return ? IteratorClose(iterated, error).
  8. If toSkip is finite and toSkip > F(2**53 - 1), then
    a. Let error be ThrowCompletion(a newly created RangeError object).
    b. Return ? IteratorClose(iterated, error).

  Each step creates the error object first and only then performs IteratorClose,
  so the error observes the error constructor that was in place *before* the
  iterator's return() method had a chance to run.
features: [iterator-includes]
---*/

var OriginalRangeError = RangeError;
var OriginalTypeError = TypeError;

// Calls Iterator.prototype.includes with skippedElements, using an iterator
// whose return() clobbers the named global error constructor. Returns the
// thrown value, and always restores the original binding.
function callWithClobberingReturn(globalName, skippedElements) {
  var Fake = function () {};
  Fake.prototype = {};

  var closed = false;
  var iterator = {
    __proto__: Iterator.prototype,
    get next() {
      throw new Test262Error('next should not be read');
    },
    return: function () {
      closed = true;
      if (globalName === 'RangeError') {
        RangeError = Fake;
      } else {
        TypeError = Fake;
      }
      return {};
    },
  };

  var threw = false;
  var thrown;
  try {
    iterator.includes(0, skippedElements);
  } catch (e) {
    threw = true;
    thrown = e;
  } finally {
    RangeError = OriginalRangeError;
    TypeError = OriginalTypeError;
  }

  return { threw: threw, thrown: thrown, closed: closed, Fake: Fake };
}

var negative = callWithClobberingReturn('RangeError', -1);
assert.sameValue(negative.threw, true, 'negative skippedElements must throw');
assert.sameValue(negative.closed, true, 'negative skippedElements must close the iterator');
assert.sameValue(
  Object.getPrototypeOf(negative.thrown),
  OriginalRangeError.prototype,
  'negative skippedElements: error must be created before IteratorClose'
);
assert.sameValue(
  Object.getPrototypeOf(negative.thrown) === negative.Fake.prototype,
  false,
  'negative skippedElements: error must not adopt the replacement prototype'
);

var tooLarge = callWithClobberingReturn('RangeError', Number.MAX_SAFE_INTEGER + 1);
assert.sameValue(tooLarge.threw, true, 'too-large skippedElements must throw');
assert.sameValue(tooLarge.closed, true, 'too-large skippedElements must close the iterator');
assert.sameValue(
  Object.getPrototypeOf(tooLarge.thrown),
  OriginalRangeError.prototype,
  'too-large skippedElements: error must be created before IteratorClose'
);

var nonIntegral = callWithClobberingReturn('TypeError', 'a string');
assert.sameValue(nonIntegral.threw, true, 'non-integral skippedElements must throw');
assert.sameValue(nonIntegral.closed, true, 'non-integral skippedElements must close the iterator');
assert.sameValue(
  Object.getPrototypeOf(nonIntegral.thrown),
  OriginalTypeError.prototype,
  'non-integral skippedElements: error must be created before IteratorClose'
);
