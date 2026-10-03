/*---
description: >
  Iterator.zipKeyed keeps the collected inner iterators and the padding values
  it reads from the padding object reachable across garbage collection while
  it is still gathering them.
esid: sec-iterator.zipkeyed
info: |
  Iterator.zipKeyed ( iterables [ , options ] )

  ...
  11. For each element key of allKeys, do
    a. Let desc be Completion(iterables.[[GetOwnProperty]](key)).
    c. If desc is not undefined and desc.[[Enumerable]] is true, then
      i. Let value be Completion(Get(iterables, key)).
      iii. If value is not undefined, then
        1. Append key to keys.
        2. Let iter be Completion(GetIteratorFlattenable(value, reject-strings)).
        4. Append iter to iters.
  ...
  13. If mode is "longest", then
    b. Else,
      i. For each element key of keys, do
        1. Let value be Completion(Get(paddingOption, key)).
        3. Append value to padding.

  Each padding value is held only by the implementation while later getters
  run user code that can trigger a collection.
includes: [compareArray.js]
features: [joint-iteration, host-gc-required]
---*/

function collect() {
  $262.gc();
  var churn = [];
  for (var i = 0; i < 500; i++) {
    churn.push({ churn: i });
  }
}

function freshIterable(values) {
  return {
    get [Symbol.iterator]() {
      collect();
      return function () {
        var index = 0;
        return {
          get next() {
            collect();
            return function () {
              collect();
              return index < values.length
                ? { done: false, value: values[index++] }
                : { done: true, value: undefined };
            };
          },
        };
      };
    },
  };
}

var iterables = {};
Object.defineProperty(iterables, "a", {
  enumerable: true,
  get: function () {
    collect();
    return freshIterable([1]);
  },
});
Object.defineProperty(iterables, "b", {
  enumerable: true,
  get: function () {
    collect();
    return freshIterable([1, 2]);
  },
});
Object.defineProperty(iterables, "c", {
  enumerable: true,
  get: function () {
    collect();
    return freshIterable([1, 2, 3]);
  },
});

var padding = {};
["a", "b", "c"].forEach(function (key) {
  Object.defineProperty(padding, key, {
    enumerable: true,
    get: function () {
      collect();
      return { pad: key };
    },
  });
});

var zipped = Iterator.zipKeyed(iterables, { mode: "longest", padding: padding });
var results = [];
for (var step = zipped.next(); !step.done; step = zipped.next()) {
  collect();
  results.push(step.value);
}

assert.sameValue(results.length, 3, "longest mode length");
assert.sameValue(results[0].a, 1, "first tuple a");
assert.sameValue(results[0].b, 1, "first tuple b");
assert.sameValue(results[0].c, 1, "first tuple c");
assert.sameValue(results[1].a.pad, "a", "second tuple a padding");
assert.sameValue(results[1].b, 2, "second tuple b");
assert.sameValue(results[1].c, 2, "second tuple c");
assert.sameValue(results[2].a.pad, "a", "third tuple a padding");
assert.sameValue(results[2].b.pad, "b", "third tuple b padding");
assert.sameValue(results[2].c, 3, "third tuple c");
