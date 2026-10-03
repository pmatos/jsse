// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-try-statement-runtime-semantics-evaluation
description: >
  A Throw or Return completion produced by the try Block of a
  try/finally statement stays reachable across a garbage collection that
  happens while the Finally Block runs, even though nothing but the
  suspended completion itself references its payload.
info: |
  TryStatement : try Block Finally

  1. Let B be the result of evaluating Block.
  2. If B.[[Type]] is normal, let F be the result of evaluating Finally.
  3. Else, let F be the result of evaluating Finally.
  4. If F.[[Type]] is normal, set F to B.
  5. Return ? UpdateEmpty(F, undefined).

  B must still exist, unmodified, when UpdateEmpty(F, undefined) runs after
  Finally evaluates.
features: [host-gc-required]
---*/

function collect() {
  $262.gc();
  var churn = [];
  for (var i = 0; i < 500; i++) {
    churn.push({ churn: i });
  }
}

// (a) Throw completion payload must survive the finally block.
var threw;
try {
  try {
    throw new Error("thrown-payload");
  } finally {
    collect();
  }
} catch (e) {
  threw = e;
}
assert.sameValue(threw.message, "thrown-payload", "Throw payload survives finally's GC");

// (b) Return completion payload must survive the finally block.
function f() {
  try {
    return { tag: "returned-payload" };
  } finally {
    collect();
  }
}
var returned = f();
assert.sameValue(returned.tag, "returned-payload", "Return payload survives finally's GC");

// (c) Same for the completion coming out of a catch block that itself runs
// before the finally.
function g() {
  try {
    throw new Error("rethrown");
  } catch (e) {
    return { tag: e.message };
  } finally {
    collect();
  }
}
var caughtThenReturned = g();
assert.sameValue(
  caughtThenReturned.tag,
  "rethrown",
  "Completion produced by catch survives finally's GC"
);
