/*---
description: >
  Iterator.prototype.reduce creates its reducer validation error before
  closing the underlying iterator, so return() cannot change the error's
  prototype by replacing the global TypeError constructor.
esid: sec-iterator.prototype.reduce
info: |
  Iterator.prototype.reduce ( reducer [ , initialValue ] )

  4. If IsCallable(reducer) is false, then
    a. Let error be ThrowCompletion(a newly created TypeError object).
    b. Return ? IteratorClose(iterated, error).
features: [iterator-helpers]
---*/

var OriginalTypeError = TypeError;
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
    TypeError = Fake;
    if (typeof $262 !== 'undefined' && $262.gc) {
      $262.gc();
    }
    return {};
  },
};

var thrown;
try {
  iterator.reduce(null);
} catch (error) {
  thrown = error;
} finally {
  TypeError = OriginalTypeError;
}

assert.sameValue(closed, true, 'validation failure must close the iterator');
assert.sameValue(
  Object.getPrototypeOf(thrown),
  OriginalTypeError.prototype,
  'error must be created before IteratorClose'
);
assert.sameValue(
  Object.getPrototypeOf(thrown) === Fake.prototype,
  false,
  'error must not adopt the replacement prototype'
);
assert.sameValue(thrown.name, 'TypeError', 'error must survive collection during IteratorClose');
assert.sameValue(typeof thrown.message, 'string', 'error message must survive collection');
