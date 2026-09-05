/*---
description: >
  Iterator.concat's return method closes the current underlying iterator even
  when a garbage collection happened since it was opened.
esid: sec-iterator.concat
info: |
  The iterator produced by Iterator.concat closes the iterator it is currently
  drawing values from. That iterator is reachable only from the helper, so a
  collection between opening it and closing it must not lose it.
features: [iterator-sequencing, iterator-helpers, host-gc-required]
---*/

var closed = 0;

var concat = Iterator.concat({
  [Symbol.iterator]: function* () {
    try {
      yield 1;
      yield 2;
    } finally {
      closed += 1;
    }
  },
});

var first = concat.next();
assert.sameValue(first.value, 1, "first value from the current iterator");
assert.sameValue(closed, 0, "current iterator is still open");

first = undefined;
$262.gc();

var result = concat.return();
assert.sameValue(result.value, undefined, "return result has no value");
assert.sameValue(result.done, true, "return result is done");
assert.sameValue(closed, 1, "current iterator was closed after collection");

var after = concat.next();
assert.sameValue(after.done, true, "helper stays exhausted after return");
assert.sameValue(closed, 1, "current iterator is not closed twice");
