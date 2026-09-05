/*---
description: >
  Iterator.concat opens each iterable exactly once and defers the TypeError for
  a non-callable next method to the step that calls it.
esid: sec-iterator.concat
info: |
  Iterator.concat ( ...items )

  ...
  3. Let closure be a new Abstract Closure with no parameters that captures
     iterables and performs the following steps when called:
    a. For each Record iterable of iterables, do
      i. Let iter be ? Call(iterable.[[OpenMethod]], iterable.[[Iterable]]).
      ii. If iter is not an Object, throw a TypeError exception.
      iii. Let iteratorRecord be ? GetIteratorDirect(iter).
      iv. Let innerAlive be true.
      v. Repeat, while innerAlive is true,
        1. Let innerValue be ? IteratorStepValue(iteratorRecord).
        ...

  GetIteratorDirect ( obj )

  1. Let nextMethod be ? Get(obj, "next").
  2. Let iteratorRecord be the Iterator Record { [[Iterator]]: obj,
     [[NextMethod]]: nextMethod, [[Done]]: false }.
  3. Return iteratorRecord.

  GetIteratorDirect does not check that nextMethod is callable, so an opened
  iterator with no next method is a valid Iterator Record. The TypeError is
  owed by IteratorStepValue, which calls it. In particular the helper must not
  treat the absent method as "no iterator open" and re-run step 3.a.i.
features: [iterator-sequencing]
---*/

var opened = 0;

var concat = Iterator.concat({
  [Symbol.iterator]: function () {
    opened += 1;
    return {};
  },
});

assert.throws(TypeError, function () {
  concat.next();
}, "calling the absent next method throws a TypeError");

assert.sameValue(opened, 1, "the iterable is opened exactly once");
