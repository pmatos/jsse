/*---
description: >
  Iterator.prototype.reduce keeps the iterator record's cached next method alive
  across the whole iteration, so a collection triggered from user code
  mid-iteration cannot reclaim it.
esid: sec-iterator.prototype.reduce
info: |
  Iterator.prototype.reduce ( reducer [ , initialValue ] )

  4. Let iterated be ? GetIteratorDirect(O).
  10. Repeat,
    a. Let value be ? IteratorStepValue(iterated).

  GetIteratorDirect reads `next` once and caches it in the Iterator Record. When
  `next` is an accessor returning a freshly created function, nothing else in the
  heap refers to that function, so an implementation that holds it only in a
  native local can lose it to a garbage collection triggered by user code running
  later in the loop.
features: [iterator-helpers]
---*/

function makeIterator() {
  var i = 0;
  return {
    __proto__: Iterator.prototype,
    // A fresh function each read: no other heap object owns it.
    get next() {
      return function () {
        i++;
        if (i > 6) {
          return { done: true, value: undefined };
        }
        return {
          done: false,
          get value() {
            if (typeof $262 !== 'undefined' && $262.gc) {
              $262.gc();
            }
            return i;
          },
        };
      };
    },
  };
}

function add(accumulator, value) {
  return accumulator + value;
}

assert.sameValue(
  makeIterator().reduce(add, 10),
  31,
  'an explicit initial value must survive iteration under collection pressure'
);

assert.sameValue(
  makeIterator().reduce(add),
  21,
  'the first iterator value must seed reduction under collection pressure'
);
