// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-iteratorstepvalue
description: >
  Generator, Map, Set and String iterators consumed through for-of, spread,
  destructuring, Array.from and the Map/Set constructors behave exactly as
  if each step went through the iterator-result object: yield* hands the
  inner result object to the consumer unchanged (its done/value are read by
  the consumer a second time), entries added or removed mid-iteration are
  seen, patched next methods apply only to later loops, and a String iterator
  steps by code point.
info: |
  7.4.8 IteratorStepValue ( iteratorRecord )
  15.5.5 yield* (sync): "Set received to Completion(GeneratorYield(innerResult))"
  24.1.5.1 CreateMapIterator, 24.2.5.1 CreateSetIterator
  22.1.5.1 CreateStringIterator
includes: [compareArray.js]
features: [Symbol.iterator, generators]
---*/

// yield* forwards the inner result object; the consumer reads it again.
(function () {
  var reads = [];
  function result(value, done) {
    return {
      get done() {
        reads.push("done:" + value);
        return done;
      },
      get value() {
        reads.push("value:" + value);
        return value;
      },
    };
  }
  var n = 0;
  var inner = {
    [Symbol.iterator]: function () {
      return this;
    },
    next: function () {
      n++;
      return n <= 2 ? result(n, false) : result("end", true);
    },
  };
  function* outer() {
    var r = yield* inner;
    return r;
  }
  var seen = [];
  for (var v of outer()) seen.push(v);
  assert.compareArray(seen, [1, 2]);
  assert.compareArray(reads, [
    "done:1", "done:1", "value:1",
    "done:2", "done:2", "value:2",
    "done:end", "value:end",
  ], "yield* and the consumer each read done; value is read once");
})();

// Generators: values, return value, exceptions and re-entrancy.
(function () {
  function* g() {
    var got = yield 1;
    yield got;
    return "ret";
  }
  var seen = [];
  for (var v of g()) seen.push(v);
  assert.compareArray(seen, [1, undefined], "for-of sends undefined, ignores return value");
  assert.compareArray([...g()], [1, undefined]);
  var [p, q, r] = g();
  assert.sameValue(r, undefined, "return value is not an element");

  function* thrower() {
    yield 1;
    throw new RangeError("boom");
  }
  var log = [];
  assert.throws(RangeError, function () {
    for (var v of thrower()) log.push(v);
  });
  assert.compareArray(log, [1]);

  var it;
  function* reenter() {
    for (var x of it) {
    }
  }
  it = reenter();
  assert.throws(TypeError, function () {
    for (var x of it) {
    }
  }, "generator already running");

  function* finallyGen() {
    try {
      yield 1;
      yield 2;
    } finally {
      log.push("finally");
    }
  }
  log = [];
  for (var v of finallyGen()) {
    log.push(v);
    break;
  }
  assert.compareArray(log, [1, "finally"], "break resumes the generator with return");
})();

// Patching %GeneratorPrototype%.next affects only generators iterated later.
(function () {
  var GeneratorPrototype = Object.getPrototypeOf(function* () {}).prototype;
  var originalNext = GeneratorPrototype.next;
  function* g() {
    yield 1;
    yield 2;
    yield 3;
  }
  var seen = [];
  var calls = 0;
  for (var v of g()) {
    seen.push(v);
    GeneratorPrototype.next = function () {
      calls++;
      return originalNext.apply(this, arguments);
    };
  }
  assert.compareArray(seen, [1, 2, 3]);
  assert.sameValue(calls, 0, "running loop keeps the captured next");
  assert.compareArray([...g()], [1, 2, 3]);
  assert.sameValue(calls, 4, "a later loop calls the replacement per step, plus done");
  GeneratorPrototype.next = originalNext;
})();

// Map and Set: mutation during iteration.
(function () {
  var m = new Map([[1, "a"], [2, "b"], [3, "c"]]);
  var seen = [];
  for (var [k, v] of m) {
    seen.push(k + v);
    if (k === 1) {
      m.delete(2);
      m.set(4, "d");
    }
  }
  assert.compareArray(seen, ["1a", "3c", "4d"]);

  var s = new Set([1, 2, 3]);
  seen = [];
  var readded = false;
  for (var v of s) {
    seen.push(v);
    if (!readded) {
      readded = true;
      s.delete(1);
      s.add(1);
    }
  }
  assert.compareArray(seen, [1, 2, 3, 1]);

  var cleared = new Set([1, 2, 3]);
  seen = [];
  for (var v of cleared) {
    seen.push(v);
    cleared.clear();
  }
  assert.compareArray(seen, [1]);

  assert.compareArray([...new Set([5, 6]).entries()][1], [6, 6]);
  assert.compareArray([...new Map([[1, 2]]).keys()], [1]);
  assert.compareArray([...new Map([[1, 2]]).values()], [2]);
  var it = new Set([1])[Symbol.iterator]();
  it.next();
  assert.sameValue(it.next().done, true);
})();

// Map/Set iterators with a replaced next, and brand checks.
(function () {
  var MapIteratorPrototype = Object.getPrototypeOf(new Map()[Symbol.iterator]());
  var original = MapIteratorPrototype.next;
  var m = new Map([[1, 1], [2, 2]]);
  var seen = [];
  for (var [k] of m) {
    seen.push(k);
    MapIteratorPrototype.next = function () {
      return { done: true };
    };
  }
  assert.compareArray(seen, [1, 2]);
  assert.compareArray([...m], []);
  MapIteratorPrototype.next = original;

  assert.throws(TypeError, function () {
    original.call({});
  });
  var fake = {
    [Symbol.iterator]: function () {
      return Object.create(MapIteratorPrototype);
    },
  };
  assert.throws(TypeError, function () {
    for (var v of fake) {
    }
  });
})();

// String iteration by code point.
(function () {
  var s = "a😀b\uD800c\uDC00";
  var seen = [];
  for (var ch of s) seen.push(ch);
  assert.compareArray(seen, ["a", "😀", "b", "\uD800", "c", "\uDC00"]);
  assert.compareArray([...s], seen);
  assert.compareArray(Array.from(s), seen);
  var [first, second] = s;
  assert.sameValue(second, "😀");
  assert.sameValue(new Set(s).size, 6, "Set constructor accepts a string");
  var StringIteratorPrototype = Object.getPrototypeOf(""[Symbol.iterator]());
  var original = StringIteratorPrototype.next;
  var out = [];
  for (var c of "xyz") {
    out.push(c);
    StringIteratorPrototype.next = function () {
      return { done: true };
    };
  }
  assert.compareArray(out, ["x", "y", "z"]);
  assert.compareArray([..."xyz"], []);
  StringIteratorPrototype.next = original;
})();

// Set and Map constructors read next once and validate the result object.
(function () {
  var nextReads = 0;
  var iterable = {
    [Symbol.iterator]: function () {
      var i = 0;
      return {
        get next() {
          nextReads++;
          return function () {
            i++;
            return { done: i > 3, value: i };
          };
        },
      };
    },
  };
  assert.sameValue(new Set(iterable).size, 3);
  assert.sameValue(nextReads, 1, "next is read once");

  var bad = {
    [Symbol.iterator]: function () {
      return {
        next: function () {
          return 1;
        },
      };
    },
  };
  assert.throws(TypeError, function () {
    new Set(bad);
  }, "Set: non-object iterator result");
  assert.throws(TypeError, function () {
    new Map(bad);
  }, "Map: non-object iterator result");
  assert.throws(TypeError, function () {
    Array.from(bad);
  }, "Array.from: non-object iterator result");

  nextReads = 0;
  assert.compareArray(Array.from(iterable), [1, 2, 3]);
  assert.sameValue(nextReads, 1, "Array.from reads next once");
})();

// Closing: a throwing adder or mapper closes the iterator exactly once.
(function () {
  var closed = 0;
  function make() {
    var i = 0;
    return {
      [Symbol.iterator]: function () {
        return this;
      },
      next: function () {
        return { done: false, value: ++i };
      },
      return: function () {
        closed++;
        return {};
      },
    };
  }
  assert.throws(Test262Error, function () {
    Array.from(make(), function () {
      throw new Test262Error();
    });
  });
  assert.sameValue(closed, 1);
  var proto = Set.prototype.add;
  Set.prototype.add = function () {
    throw new Test262Error();
  };
  try {
    assert.throws(Test262Error, function () {
      new Set(make());
    });
  } finally {
    Set.prototype.add = proto;
  }
  assert.sameValue(closed, 2);
})();
