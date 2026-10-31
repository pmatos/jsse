// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-iteratorprototype.find
description: >
  Iterator.prototype.some, every, find and the other consuming helpers keep
  the value they are about to return or rethrow alive while IteratorClose runs
  the iterator's return method, which can trigger a garbage collection.
info: |
  %Iterator.prototype%.find ( predicate )

  6.e. Let result be Completion(Call(predicate, undefined, « value, 𝔽(counter) »)).
  6.f. IfAbruptCloseIterator(result, iterated).
  6.g. If ToBoolean(result) is true, return ? IteratorClose(iterated, NormalCompletion(value)).

  IteratorClose ( iteratorRecord, completion )

  3. Let innerResult be Completion(GetMethod(iterator, "return")).
  4. If innerResult is a normal completion, then
    a. Let return be innerResult.[[Value]].
    c. Set innerResult to Completion(Call(return, iterator)).
  5. If completion is a throw completion, return ? completion.
features: [iterator-helpers, host-gc-required]
---*/

function collect() {
  $262.gc();
  var churn = [];
  for (var i = 0; i < 500; i++) {
    churn.push({ churn: i });
  }
}

class CollectingIterator extends Iterator {
  next() {
    return { done: false, value: { marker: "found" } };
  }
  return() {
    collect();
    return {};
  }
}

assert.throws(Test262Error, function() {
  new CollectingIterator().find(function() {
    throw new Test262Error("predicate threw");
  });
}, "find rethrows the predicate's error after closing");

assert.throws(Test262Error, function() {
  new CollectingIterator().some(function() {
    throw new Test262Error("predicate threw");
  });
}, "some rethrows the predicate's error after closing");

assert.throws(Test262Error, function() {
  new CollectingIterator().every(function() {
    throw new Test262Error("predicate threw");
  });
}, "every rethrows the predicate's error after closing");

assert.throws(Test262Error, function() {
  new CollectingIterator().forEach(function() {
    throw new Test262Error("callback threw");
  });
}, "forEach rethrows the callback's error after closing");

var found = new CollectingIterator().find(function() {
  return true;
});

assert.sameValue(typeof found, "object", "find returns an object after closing");
assert.sameValue(found.marker, "found", "find returns the matched value, not a freed slot");

var mapped = new CollectingIterator().map(function() {
  throw new Test262Error("mapper threw");
});
assert.throws(Test262Error, function() {
  mapped.next();
}, "map rethrows the mapper's error after closing");

assert.throws(Test262Error, function() {
  new CollectingIterator().take({
    valueOf: function() {
      throw new Test262Error("limit threw");
    }
  });
}, "take rethrows a limit coercion error after closing");

assert.throws(Test262Error, function() {
  new CollectingIterator().drop({
    valueOf: function() {
      throw new Test262Error("limit threw");
    }
  });
}, "drop rethrows a limit coercion error after closing");
