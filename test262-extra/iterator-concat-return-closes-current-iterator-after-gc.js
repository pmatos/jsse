/*---
description: >
  Iterator.concat's return method closes the current underlying iterator even
  when a garbage collection happened since it was opened.
esid: sec-iterator.concat
info: |
  Iterator.concat ( ...items )

  ...
  3. Let closure be a new Abstract Closure ... :
    a. For each Record iterable of iterables, do
      v. Repeat, while innerAlive is true,
        3. Else,
          a. Let completion be Completion(Yield(innerValue)).
          b. If completion is an abrupt completion, then
            i. Return ? IteratorClose(iteratorRecord, completion).
  ...

  Calling the helper's return method resumes the generator with an abrupt
  completion, so step 3.a.v.3.b.i closes the iteratorRecord it is currently
  drawing from. A collection between opening that iterator and closing it must
  not lose it, and it must be closed exactly once.
features: [iterator-sequencing, host-gc-required]
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
