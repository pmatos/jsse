/*---
description: >
  Iterator.zip keeps the input iterator, the collected inner iterators and the
  collected padding values reachable across garbage collection while it is
  still reading them from its arguments.
esid: sec-iterator.zip
info: |
  Iterator.zip ( iterables [ , options ] )

  ...
  10. Let inputIter be ? GetIterator(iterables, sync).
  12. Repeat, while next is not done,
    a. Set next to Completion(IteratorStepValue(inputIter)).
    c. Let iter be Completion(GetIteratorFlattenable(next, reject-strings)).
    e. Append iter to iters.
  ...
  14. If mode is "longest", then
    a. If paddingOption is undefined, ...
    b. Else,
      i. Let paddingIter be ? GetIterator(paddingOption, sync).
      iii. Repeat, while i < iterCount,
        1. If usingIterator is true, then
          a. Set next to Completion(IteratorStepValue(paddingIter)).
          d. Else, append next to padding.

  Every iterator record and padding value is held only by the implementation
  while later iterable, iterator and padding steps run user code that can
  trigger a collection.
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
    [Symbol.iterator]: function () {
      var index = 0;
      return {
        next: function () {
          collect();
          return index < values.length
            ? { done: false, value: values[index++] }
            : { done: true, value: undefined };
        },
      };
    },
  };
}

function inputIterator(count, makeItem) {
  return {
    [Symbol.iterator]: function () {
      var produced = 0;
      return {
        next: function () {
          return {
            get done() {
              collect();
              return produced >= count;
            },
            get value() {
              collect();
              return makeItem(produced++);
            },
          };
        },
      };
    },
  };
}

var shortest = Iterator.zip(
  inputIterator(3, function (k) {
    return freshIterable([k, k + 10]);
  })
);
var shortestResults = [];
for (var step = shortest.next(); !step.done; step = shortest.next()) {
  shortestResults.push(step.value.slice());
}
assert.sameValue(shortestResults.length, 2, "shortest mode length");
assert.compareArray(shortestResults[0], [0, 1, 2], "shortest first tuple");
assert.compareArray(shortestResults[1], [10, 11, 12], "shortest second tuple");

var padIterable = {
  [Symbol.iterator]: function () {
    var produced = 0;
    return {
      next: function () {
        return {
          get done() {
            collect();
            return produced >= 3;
          },
          get value() {
            collect();
            return { pad: produced++ };
          },
        };
      },
    };
  },
};

var longest = Iterator.zip(
  [[1], [1, 2], [1, 2, 3]],
  { mode: "longest", padding: padIterable }
);
var longestResults = [];
for (var step2 = longest.next(); !step2.done; step2 = longest.next()) {
  longestResults.push(step2.value.slice());
}
assert.sameValue(longestResults.length, 3, "longest mode length");
assert.compareArray(longestResults[0], [1, 1, 1], "longest first tuple");
assert.sameValue(longestResults[1][0].pad, 0, "longest second tuple first padding");
assert.sameValue(longestResults[1][1], 2, "longest second tuple second value");
assert.sameValue(longestResults[1][2], 2, "longest second tuple third value");
assert.sameValue(longestResults[2][0].pad, 0, "longest third tuple first padding");
assert.sameValue(longestResults[2][1].pad, 1, "longest third tuple second padding");
assert.sameValue(longestResults[2][2], 3, "longest third tuple third value");

var strict = Iterator.zip(
  inputIterator(2, function (k) {
    return freshIterable([k, k]);
  }),
  { mode: "strict" }
);
assert.compareArray(strict.next().value, [0, 1], "strict first tuple");
