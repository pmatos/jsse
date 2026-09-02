/*---
description: >
  Iterator.prototype.flatMap keeps the current inner iterator reachable across
  garbage collection between calls to the helper's next method.
esid: sec-iterator.prototype.flatmap
info: |
  Iterator.prototype.flatMap repeatedly resumes the current inner iterator
  until it is exhausted. The inner iterator must remain strongly reachable
  while the helper is live, including across separate calls to the helper's
  next method.
features: [iterator-helpers, host-gc-required]
---*/

var flat = (function* () {
  yield 1;
  yield 2;
})().flatMap(function* (value) {
  yield value;
  yield value * 10;
});

var first = flat.next();
assert.sameValue(first.value, 1, "first value from the first inner iterator");
assert.sameValue(first.done, false, "first inner iterator is still active");

first = undefined;
$262.gc();

var second = flat.next();
assert.sameValue(second.value, 10, "current inner iterator survives collection");
assert.sameValue(second.done, false, "current inner iterator resumes after collection");

second = undefined;
$262.gc();

var third = flat.next();
assert.sameValue(third.value, 2, "next inner iterator opens after collection");
assert.sameValue(third.done, false, "last inner iterator is active");

third = undefined;
$262.gc();

var fourth = flat.next();
assert.sameValue(fourth.value, 20, "last inner iterator survives collection");
assert.sameValue(fourth.done, false, "last inner iterator resumes after collection");

var done = flat.next();
assert.sameValue(done.value, undefined, "exhausted helper has no value");
assert.sameValue(done.done, true, "helper finishes after the last inner iterator");
