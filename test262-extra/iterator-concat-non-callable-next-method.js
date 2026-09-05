/*---
description: >
  Iterator.concat opens each iterable exactly once and defers the TypeError for
  a non-callable next method to the step that calls it.
esid: sec-iterator.concat
info: |
  Iterator.concat ( ...items )

  ...
      iii. Let iteratorRecord be ? GetIteratorDirect(iter).
      v. Repeat, while innerAlive is true,
        1. Let innerValue be ? IteratorStepValue(iteratorRecord).
  ...

  GetIteratorDirect ( obj )

  1. Let nextMethod be ? Get(obj, "next").
  ...

  GetIteratorDirect does not check that nextMethod is callable, so an opened
  iterator with no next method is a valid Iterator Record and the TypeError is
  owed by IteratorStepValue. In particular the helper must not read the absent
  method as "no iterator open" and re-open the same iterable.
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
