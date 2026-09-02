/*---
description: >
  Iterator.prototype.take creates limit validation errors before closing the
  underlying iterator, so return() cannot change their prototype by replacing
  the global RangeError constructor.
esid: sec-iterator.prototype.take
info: |
  Iterator.prototype.take ( limit )

  6. If numLimit is NaN, then
    a. Let error be ThrowCompletion(a newly created RangeError object).
    b. Return ? IteratorClose(iterated, error).
  7. If numLimit is finite and numLimit > F(2**53 - 1), then
    a. Let error be ThrowCompletion(a newly created RangeError object).
    b. Return ? IteratorClose(iterated, error).
  9. If integerLimit < 0, then
    a. Let error be ThrowCompletion(a newly created RangeError object).
    b. Return ? IteratorClose(iterated, error).
features: [iterator-helpers]
---*/

var OriginalRangeError = RangeError;

function callWithClobberingReturn(limit) {
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
      RangeError = Fake;
      if (typeof $262 !== 'undefined' && $262.gc) {
        $262.gc();
      }
      return {};
    },
  };

  var thrown;
  try {
    iterator.take(limit);
  } catch (error) {
    thrown = error;
  } finally {
    RangeError = OriginalRangeError;
  }

  return { thrown: thrown, closed: closed, Fake: Fake };
}

function assertPreservedLimitError(result, description) {
  assert.sameValue(result.closed, true, description + ': validation failure must close');
  assert.sameValue(
    Object.getPrototypeOf(result.thrown),
    OriginalRangeError.prototype,
    description + ': error must be created before IteratorClose'
  );
  assert.sameValue(
    Object.getPrototypeOf(result.thrown) === result.Fake.prototype,
    false,
    description + ': error must not adopt the replacement prototype'
  );
  assert.sameValue(
    result.thrown.name,
    'RangeError',
    description + ': error must survive collection during IteratorClose'
  );
  assert.sameValue(
    typeof result.thrown.message,
    'string',
    description + ': error message must survive collection'
  );
}

assertPreservedLimitError(callWithClobberingReturn(NaN), 'NaN limit');
assertPreservedLimitError(
  callWithClobberingReturn(Number.MAX_SAFE_INTEGER + 1),
  'too-large limit'
);
assertPreservedLimitError(callWithClobberingReturn(-1), 'negative limit');
