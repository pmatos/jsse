// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  The value a for-of loop threads through its *normal* completion (no
  break/return/continue — the iterator simply reports done) stays reachable
  across the garbage collection that can happen while a later iteration's
  next() runs, even though nothing but the loop's own running completion
  value references it. Only observable through Eval's completion value
  (UpdateEmpty), so this is driven through `eval`.
info: |
  ForIn/OfBodyEvaluation ( lhs, stmt, iteratorRecord, iterationKind,
  lhsKind, labelSet [ , iteratorKind ] )

  Repeat,
    ...
    Let result be Completion(Evaluation of stmt).
    ...
    If result.[[Value]] is not empty, set V to result.[[Value]].

  V is carried across every subsequent iteration's IteratorStep (which can
  run arbitrary user code via next()) until the loop finally returns V on a
  normal (iterator-exhausted) exit.
features: [host-gc-required]
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
            // Runs on the final `next()` call, after the loop body has
            // already produced its last Normal completion value.
            collect();
            return { done: true };
          }
          return { done: false, value: i };
        },
      };
    },
  };
}

var normal = eval(
  "for (const x of makeIterable(3)) { ({ tag: 'normal-payload-' + x, arr: new Array(50).fill(x) }); }"
);
assert.sameValue(
  normal.tag,
  "normal-payload-3",
  "the loop's running completion value survives the exhausting next() call's garbage collection"
);
assert.sameValue(
  normal.arr.length,
  50,
  "the completion value's own contents are not corrupted by the collection"
);
