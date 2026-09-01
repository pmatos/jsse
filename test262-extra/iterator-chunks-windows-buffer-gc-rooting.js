/*---
description: >
  Iterator chunking helpers keep buffered values reachable while JavaScript
  execution between iterator steps triggers garbage collection.
esid: sec-iterator.prototype.chunks
info: |
  Iterator.prototype.chunks and Iterator.prototype.windows accumulate values
  in an internal List before CreateArrayFromList exposes them. Values in that
  List must remain strongly reachable while the helper is live, including
  across arbitrary JavaScript execution in a later iterator step and across
  separate calls to the helper's next method.
features: [iterator-chunking, host-gc-required]
---*/

function collectingIterator(prefix, count, collectBefore) {
  var index = 0;
  return {
    __proto__: Iterator.prototype,
    next: function () {
      ++index;
      if (index === collectBefore) {
        $262.gc();
      }
      if (index > count) {
        return { done: true };
      }
      return {
        done: false,
        value: { marker: prefix + index },
      };
    },
  };
}

var chunk = collectingIterator("chunk-", 3, 2).chunks(3).next().value;
assert.sameValue(chunk[0].marker, "chunk-1", "first chunk value survives collection");
assert.sameValue(chunk[1].marker, "chunk-2", "second chunk value is preserved");
assert.sameValue(chunk[2].marker, "chunk-3", "third chunk value is preserved");

var windowed = collectingIterator("window-", 3, 2).windows(2);
var firstWindow = windowed.next().value;
assert.sameValue(
  firstWindow[0].marker,
  "window-1",
  "first window value survives collection during buffer fill"
);
assert.sameValue(firstWindow[1].marker, "window-2", "first window is complete");

firstWindow = undefined;
$262.gc();

var secondWindow = windowed.next().value;
assert.sameValue(
  secondWindow[0].marker,
  "window-2",
  "retained window value survives collection between helper calls"
);
assert.sameValue(secondWindow[1].marker, "window-3", "sliding window is complete");
