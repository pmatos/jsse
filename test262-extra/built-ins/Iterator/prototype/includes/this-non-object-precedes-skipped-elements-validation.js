/*---
description: >
  Iterator.prototype.includes validates its receiver before validating
  skippedElements.
esid: sec-iterator.prototype.includes
info: |
  Iterator.prototype.includes ( searchElement [ , skippedElements ] )

  1. Let O be the this value.
  2. If O is not an Object, throw a TypeError exception.
  ...
  8. If toSkip is finite and toSkip > F(2**53 - 1), then
    a. Let error be ThrowCompletion(a newly created RangeError object).
    b. Return ? IteratorClose(iterated, error).
features: [iterator-includes]
---*/

assert.sameValue(
  typeof Iterator.prototype.includes,
  "function",
  "includes must be installed before its receiver validation can be observed"
);

assert.throws(TypeError, function () {
  Iterator.prototype.includes.call(
    null,
    0,
    Number.MAX_SAFE_INTEGER + 1
  );
});
