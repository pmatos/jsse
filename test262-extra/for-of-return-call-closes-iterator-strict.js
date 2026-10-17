/*---
description: A call returned from a for-of body is not in tail position, so the loop's iterator is closed
esid: sec-static-semantics-hascallintailposition
info: |
  HasCallInTailPosition has no clause for iteration statements, so a call in a
  for-of body is never in tail position. `return f()` out of the loop performs
  IteratorClose (14.7.5.7 ForIn/OfBodyEvaluation) before the function completes.
flags: [onlyStrict]
features: [tail-call-optimization]
---*/

"use strict";

var closed = 0;
var iterable = {
  [Symbol.iterator]() {
    return {
      next() { return { value: 1, done: false }; },
      return() { closed++; return {}; }
    };
  }
};

function leaf() { return "leaf"; }

// A direct eval keeps the function body on the tree-walker in every execution mode.
function viaEval() {
  eval("");
  for (var x of iterable) { return leaf(); }
}

function plain() {
  for (var x of iterable) { return leaf(); }
}

function nested() {
  outer: for (var a of iterable) {
    for (var b of iterable) { return leaf(); }
  }
}

function insideTry() {
  eval("");
  for (var x of iterable) {
    try { return leaf(); } finally { }
  }
}

assert.sameValue(viaEval(), "leaf", "viaEval result");
assert.sameValue(closed, 1, "return of a call closes the iterator (tree-walker body)");

assert.sameValue(plain(), "leaf", "plain result");
assert.sameValue(closed, 2, "return of a call closes the iterator");

assert.sameValue(nested(), "leaf", "nested result");
assert.sameValue(closed, 4, "both iterators are closed");

assert.sameValue(insideTry(), "leaf", "insideTry result");
assert.sameValue(closed, 5, "return of a call from try inside for-of closes the iterator");

function count(n, acc) {
  if (n <= 0) return acc;
  return count(n - 1, acc + 1);
}

assert.sameValue(count(200000, 0), 200000, "a proper tail call outside a for-of remains optimized");
