// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-iteratorbindinginitialization
description: >
  Each element already collected by an array destructuring pattern's rest
  element (`var [...rest] = iterable`) stays reachable across the garbage
  collection that can happen while a later element is being pulled from
  the same iterator, even though the accumulating list is a plain internal
  list with no reference from any JavaScript-visible value yet.
info: |
  Runtime Semantics: IteratorBindingInitialization
  BindingRestElement : ... BindingIdentifier

  Repeat,
    Let next be ? IteratorStepValue(iteratorRecord).
    If next is not DONE, append next to A.

  Every IteratorStepValue after the first can run arbitrary user code via
  next(), which can free an earlier, not-yet-JS-visible element already
  appended to A.
features: [destructuring-binding, host-gc-required]
---*/

function collect() {
  $262.gc();
  var churn = [];
  for (var i = 0; i < 500; i++) {
    churn.push({ churn: i });
  }
}

function makeIterable(count) {
  return {
    [Symbol.iterator]() {
      var i = 0;
      return {
        next() {
          i++;
          if (i > count) {
            return { done: true };
          }
          // Collect on every step after the first, so each already-collected
          // rest element is exercised.
          if (i > 1) {
            collect();
          }
          return { done: false, value: { tag: "elem-" + i, arr: new Array(20).fill(i) } };
        },
      };
    },
  };
}

var [...r] = makeIterable(5);
assert.sameValue(r.length, 5, "all 5 elements are collected into the rest array");
for (var i = 0; i < 5; i++) {
  assert.sameValue(
    r[i].tag,
    "elem-" + (i + 1),
    "rest element " + i + " survives the collections triggered by later next() calls"
  );
  assert.sameValue(
    r[i].arr.length,
    20,
    "rest element " + i + "'s own contents are not corrupted by a later collection"
  );
}
