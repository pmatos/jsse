/*---
description: >
  Iterator.prototype.includes keeps the iterator record's cached next method
  alive across the whole iteration, so a collection triggered from user code
  mid-iteration cannot reclaim it.
esid: sec-iterator.prototype.includes
info: |
  Iterator.prototype.includes ( searchElement [ , skippedElements ] )

  10. Set iterated to ? GetIteratorDirect(O).
  11. Repeat,
    a. Let next be ? IteratorStepValue(iterated).

  GetIteratorDirect reads `next` once and caches it in the Iterator Record. When
  `next` is an accessor returning a freshly created function, nothing else in the
  heap refers to that function, so an implementation that holds it only in a
  native local can lose it to a garbage collection triggered by user code running
  later in the loop.
features: [iterator-includes]
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

assert.sameValue(
  makeIterator().includes(5),
  true,
  'a collection during iteration must not reclaim the cached next method'
);

assert.sameValue(
  makeIterator().includes(99),
  false,
  'exhausting the iterator under collection pressure must still return false'
);

assert.sameValue(
  makeIterator().includes(5, 2),
  true,
  'skipping under collection pressure must still find a later match'
);
