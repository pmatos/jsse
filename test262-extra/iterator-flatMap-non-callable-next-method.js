/*---
description: >
  Iterator.prototype.flatMap throws a TypeError when the inner iterator has no
  callable next method, rather than treating it as exhausted.
esid: sec-iterator.prototype.flatmap
info: |
  Iterator.prototype.flatMap ( mapper )

  ...
    vi. Let innerIterator be Completion(GetIteratorFlattenable(mapped,
        reject-primitives)).
    ...
    ix. Repeat, while innerAlive is true,
      1. Let innerValue be Completion(IteratorStepValue(innerIterator)).
      ...

  GetIteratorDirect ( obj )

  1. Let nextMethod be ? Get(obj, "next").
  2. Let iteratorRecord be the Iterator Record { [[Iterator]]: obj,
     [[NextMethod]]: nextMethod, [[Done]]: false }.
  3. Return iteratorRecord.

  GetIteratorFlattenable does not check that nextMethod is callable, so the
  inner iterator is a valid Iterator Record and the TypeError is owed by
  IteratorStepValue. The helper must not read the absent method as "no inner
  iterator open" and silently move on to the next outer value.
features: [iterator-helpers]
---*/

var flat = [1, 2].values().flatMap(function () {
  return {
    [Symbol.iterator]: function () {
      return {};
    },
  };
});

assert.throws(TypeError, function () {
  flat.next();
}, "calling the absent next method throws a TypeError");
