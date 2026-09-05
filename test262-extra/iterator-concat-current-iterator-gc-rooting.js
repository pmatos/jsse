/*---
description: >
  Iterator.concat keeps the current underlying iterator reachable across
  garbage collection between calls to the helper's next method.
esid: sec-iterator.concat
info: |
  Iterator.concat opens each iterable in turn and resumes the resulting
  iterator until it is exhausted. That iterator is reachable only from the
  helper, so it must stay strongly rooted while the helper is live, including
  across separate calls to the helper's next method.
features: [iterator-sequencing, iterator-helpers, host-gc-required]
---*/

var concat = Iterator.concat(
  {
    [Symbol.iterator]: function* () {
      yield 1;
      yield 2;
    },
  },
  {
    [Symbol.iterator]: function* () {
      yield 3;
      yield 4;
    },
  }
);

var first = concat.next();
assert.sameValue(first.value, 1, "first value from the first iterable");
assert.sameValue(first.done, false, "first iterator is still active");

first = undefined;
$262.gc();

var second = concat.next();
assert.sameValue(second.value, 2, "current iterator survives collection");
assert.sameValue(second.done, false, "current iterator resumes after collection");

second = undefined;
$262.gc();

var third = concat.next();
assert.sameValue(third.value, 3, "next iterable opens after collection");
assert.sameValue(third.done, false, "last iterator is active");

third = undefined;
$262.gc();

var fourth = concat.next();
assert.sameValue(fourth.value, 4, "last iterator survives collection");
assert.sameValue(fourth.done, false, "last iterator resumes after collection");

var done = concat.next();
assert.sameValue(done.value, undefined, "exhausted helper has no value");
assert.sameValue(done.done, true, "helper finishes after the last iterable");
