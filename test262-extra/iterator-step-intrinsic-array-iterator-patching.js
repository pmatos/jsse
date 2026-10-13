// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-iteratorstepvalue
description: >
  Iterating an Array through for-of, spread, array destructuring and
  Array.from runs the [[NextMethod]] captured by GetIterator. Replacing
  %ArrayIteratorPrototype%.next or Array.prototype[@@iterator] while a loop
  runs affects only loops that start afterwards, and every other observable
  step of the array iterator (element reads through accessors and prototypes,
  length changes, IteratorClose) is kept.
info: |
  7.4.3 GetIteratorFromMethod
    4. Let nextMethod be ? Get(iterator, "next").
    5. Let iteratorRecord be the Iterator Record { [[Iterator]]: iterator,
       [[NextMethod]]: nextMethod, [[Done]]: false }.
  7.4.8 IteratorStepValue ( iteratorRecord )
  23.1.5.1 CreateArrayIterator ( array, kind )
includes: [compareArray.js]
features: [Symbol.iterator]
---*/

var ArrayIteratorPrototype = Object.getPrototypeOf([][Symbol.iterator]());
var originalNext = ArrayIteratorPrototype.next;
var originalIterator = Array.prototype[Symbol.iterator];

function restore() {
  ArrayIteratorPrototype.next = originalNext;
  Array.prototype[Symbol.iterator] = originalIterator;
  delete ArrayIteratorPrototype.return;
}

// Replacing %ArrayIteratorPrototype%.next mid-loop: the running loop keeps the
// captured method, a loop started afterwards uses the replacement.
(function () {
  var seen = [];
  var calls = 0;
  for (var v of [1, 2, 3, 4]) {
    seen.push(v);
    if (v === 2) {
      ArrayIteratorPrototype.next = function () {
        calls++;
        return { done: true, value: undefined };
      };
    }
  }
  assert.compareArray(seen, [1, 2, 3, 4], "running for-of keeps the captured next");
  assert.sameValue(calls, 0, "replacement not called by the running loop");

  var after = [];
  for (var w of [7, 8]) after.push(w);
  assert.compareArray(after, [], "a new loop calls the replacement");
  assert.sameValue(calls, 1, "replacement called by the new loop");
  assert.compareArray([...[5, 6]], [], "spread uses the replacement");
  var [first = "dflt"] = [9];
  assert.sameValue(first, "dflt", "destructuring uses the replacement");
  assert.compareArray(Array.from([5, 6]), [], "Array.from uses the replacement");
  restore();
})();

// Replacing Array.prototype[@@iterator] mid-loop: the iterator already exists.
(function () {
  var seen = [];
  var replaced = 0;
  for (var v of [10, 20, 30]) {
    seen.push(v);
    Array.prototype[Symbol.iterator] = function () {
      replaced++;
      return originalIterator.call(["x"]);
    };
  }
  assert.compareArray(seen, [10, 20, 30]);
  assert.sameValue(replaced, 0);
  var viaNew = [];
  for (var w of [1, 2, 3]) viaNew.push(w);
  assert.compareArray(viaNew, ["x"], "a new loop calls the replaced @@iterator");
  assert.sameValue(replaced, 1);
  restore();
})();

// Mutating the array during iteration.
(function () {
  var a = [1, 2];
  var seen = [];
  for (var v of a) {
    seen.push(v);
    if (a.length < 5) a.push(a.length + 1);
  }
  assert.compareArray(seen, [1, 2, 3, 4, 5], "push extends iteration");

  var b = [1, 2, 3, 4, 5];
  seen = [];
  for (var v of b) {
    seen.push(v);
    if (v === 2) b.length = 3;
  }
  assert.compareArray(seen, [1, 2, 3], "truncating length ends iteration early");

  var c = [1, , 3];
  Array.prototype[1] = "proto";
  seen = [];
  for (var v of c) seen.push(v);
  delete Array.prototype[1];
  assert.compareArray(seen, [1, "proto", 3], "a hole reads through the prototype");

  seen = [];
  for (var v of [1, , 3]) seen.push(v);
  assert.compareArray(seen, [1, undefined, 3], "a hole reads undefined");

  var d = [0, 1, 2, 3];
  var getterCalls = 0;
  seen = [];
  for (var v of d) {
    seen.push(v);
    if (v === 0) {
      Object.defineProperty(d, 2, {
        get: function () {
          getterCalls++;
          return "got";
        },
        configurable: true,
      });
    }
  }
  assert.compareArray(seen, [0, 1, "got", 3], "index turned into accessor mid-loop");
  assert.sameValue(getterCalls, 1);

  var e = [1, 2, 3];
  var out = [];
  for (var v of e) {
    out.push(v);
    e[1] = "changed";
  }
  assert.compareArray(out, [1, "changed", 3], "element overwritten ahead of the cursor");

  var f = [1, 2, 3];
  Object.freeze(f);
  out = [];
  for (var v of f) out.push(v);
  assert.compareArray(out, [1, 2, 3], "frozen array");

  var g = [1, 2, 3];
  Object.defineProperty(g, 1, { value: "ro", writable: false });
  out = [];
  for (var v of g) out.push(v);
  assert.compareArray(out, [1, "ro", 3], "non-writable element");
})();

// An Array Iterator ignores its array once exhausted.
(function () {
  var a = [1];
  var it = a[Symbol.iterator]();
  assert.sameValue(it.next().done, false);
  assert.sameValue(it.next().done, true);
  a.push(2);
  assert.sameValue(it.next().done, true, "exhausted iterators stay exhausted");
  assert.sameValue(it.next().value, undefined);
})();

// IteratorClose: break calls a user-added return found up the prototype chain.
(function () {
  var closed = 0;
  var thisValue;
  ArrayIteratorPrototype.return = function () {
    closed++;
    thisValue = this;
    return {};
  };
  for (var v of [1, 2, 3]) {
    if (v === 2) break;
  }
  assert.sameValue(closed, 1, "return called once on break");
  assert.sameValue(Object.getPrototypeOf(thisValue), ArrayIteratorPrototype);

  var [x] = [1, 2, 3];
  assert.sameValue(closed, 2, "array destructuring closes an unfinished iterator");

  var [a, b, c] = [1, 2, 3];
  assert.sameValue(closed, 3, "an iterator that has not reported done is closed");
  var [d, e, f, g] = [1, 2, 3];
  assert.sameValue(closed, 3, "an exhausted iterator is not closed");
  restore();
})();

// An object with %ArrayIteratorPrototype% as prototype is not an Array Iterator.
(function () {
  var fake = {
    [Symbol.iterator]: function () {
      return Object.create(ArrayIteratorPrototype);
    },
  };
  assert.throws(TypeError, function () {
    for (var v of fake) {
    }
  }, "for-of");
  assert.throws(TypeError, function () {
    [...fake];
  }, "spread");
  assert.throws(TypeError, function () {
    var [a] = fake;
  }, "destructuring");
  assert.throws(TypeError, function () {
    Array.from(fake);
  }, "Array.from");
  assert.throws(TypeError, function () {
    new Set(fake);
  }, "Set constructor");
})();

// next borrowed from an Array Iterator and called through a different object.
(function () {
  var source = [1, 2][Symbol.iterator]();
  var proxy = {
    [Symbol.iterator]: function () {
      return { next: originalNext.bind(source) };
    },
  };
  assert.compareArray([...proxy], [1, 2]);
})();

// Typed arrays and keys/entries iterators.
(function () {
  var ta = new Uint8Array([5, 6, 7]);
  var seen = [];
  for (var v of ta) seen.push(v);
  assert.compareArray(seen, [5, 6, 7], "typed array values");
  assert.compareArray([...ta.keys()], [0, 1, 2], "typed array keys");
  var pairs = [...ta.entries()];
  assert.sameValue(pairs.length, 3);
  assert.compareArray(pairs[2], [2, 7], "typed array entries");
  assert.compareArray([...["a", "b"].keys()], [0, 1], "array keys");
  var entries = [...["a", "b"].entries()];
  assert.compareArray(entries[1], [1, "b"], "array entries");
  assert.compareArray(Array.from(new Uint8Array([1, 2]).values()), [1, 2]);

  var shrinking = new Uint8Array(new ArrayBuffer(4, { maxByteLength: 8 }));
  var count = 0;
  assert.throws(TypeError, function () {
    for (var v of shrinking) {
      count++;
      shrinking.buffer.resize(0);
      Object.defineProperty(shrinking, "x", { value: 1, configurable: true });
      // detach by transfer so the next step observes the out-of-bounds view
      shrinking.buffer.transfer();
    }
  });
  assert.sameValue(count, 1);
})();

// Array-likes and proxies go through Array.prototype.values generically.
(function () {
  var arrayLike = { length: 3, 0: "a", 1: "b", 2: "c" };
  assert.compareArray([...Array.prototype.values.call(arrayLike)], ["a", "b", "c"]);
  var log = [];
  var proxy = new Proxy([1, 2], {
    get: function (target, key, receiver) {
      log.push(typeof key === "symbol" ? "@@" : String(key));
      return Reflect.get(target, key, receiver);
    },
  });
  var seen = [];
  for (var v of proxy) seen.push(v);
  assert.compareArray(seen, [1, 2]);
  assert.compareArray(
    log,
    ["@@", "length", "0", "length", "1", "length"],
    "proxy reads: @@iterator, then length and the index per step"
  );
})();
