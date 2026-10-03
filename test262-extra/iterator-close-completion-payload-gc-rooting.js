// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-iteratorclose
description: >
  The abrupt completion passed to IteratorClose stays reachable across the
  garbage collection that can happen while `return()` runs, even though
  nothing but the in-flight completion itself references its payload.
info: |
  IteratorClose ( iteratorRecord, completion )

  3. Let innerResult be Completion(GetMethod(iterator, "return")).
  ...
  6. If innerResult.[[Type]] is normal, set innerResult to
     Completion(Call(innerResult.[[Value]], iterator)).
  7. If completion.[[Type]] is throw, return ? completion.

  `completion` must be the same value both before and after step 6's call,
  which can run arbitrary user code.
features: [host-gc-required]
---*/

function collect() {
  $262.gc();
  var churn = [];
  for (var i = 0; i < 500; i++) {
    churn.push({ churn: i });
  }
}

function makeIterable(count, onReturn) {
  return {
    [Symbol.iterator]() {
      var i = 0;
      return {
        next() {
          if (i >= count) {
            return { done: true, value: undefined };
          }
          i++;
          return { done: false, value: i };
        },
        return(v) {
          collect();
          if (onReturn) onReturn();
          return { done: true };
        },
      };
    },
  };
}

// (i) A for-of body throw is the completion IteratorClose must preserve
// across a churning return().
var thrown;
try {
  for (const x of makeIterable(5)) {
    throw new Error("body-throw-payload");
  }
} catch (e) {
  thrown = e;
}
assert.sameValue(
  thrown.message,
  "body-throw-payload",
  "for-of body throw payload survives IteratorClose's return() call"
);

// (ii) Array.from's mapFn throw is the completion IteratorClose must
// preserve across a churning return().
var fromThrown;
try {
  Array.from(makeIterable(5), function () {
    throw new Error("map-fn-throw-payload");
  });
} catch (e) {
  fromThrown = e;
}
assert.sameValue(
  fromThrown.message,
  "map-fn-throw-payload",
  "Array.from mapFn throw payload survives IteratorClose's return() call"
);

// (iii) new Map()'s entry-read throw is the completion IteratorClose must
// preserve across a churning return().
function makeEntryIterable() {
  return {
    [Symbol.iterator]() {
      var i = 0;
      return {
        next() {
          i++;
          if (i === 1) {
            return { done: false, value: [1, "a"] };
          }
          return {
            done: false,
            value: {
              get 0() {
                throw new Error("map-adder-throw-payload");
              },
            },
          };
        },
        return(v) {
          collect();
          return { done: true };
        },
      };
    },
  };
}
var mapThrown;
try {
  new Map(makeEntryIterable());
} catch (e) {
  mapThrown = e;
}
assert.sameValue(
  mapThrown.message,
  "map-adder-throw-payload",
  "Map constructor's entry-read throw payload survives IteratorClose's return() call"
);
