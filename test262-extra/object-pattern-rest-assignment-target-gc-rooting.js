// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-restdestructuringassignmentevaluation
description: >
  The fresh rest object built by an object assignment pattern's rest
  property (`({...base().k} = source)`) stays reachable across the garbage
  collection that can happen while the assignment target's own base
  expression is evaluated, even though the rest object has no
  JavaScript-visible reference to it yet.
info: |
  AssignmentRestProperty : ... DestructuringAssignmentTarget

  1. Let lRef be ? Evaluation of DestructuringAssignmentTarget.
  2. Let restObj be OrdinaryObjectCreate(%Object.prototype%).
  3. Perform ? CopyDataProperties(restObj, value, excludedNames).
  4. Return ? PutValue(lRef, restObj).

  Unlike this clause's own evaluation order (lRef resolved before restObj is
  created), this engine's DestructuringAssignmentTarget evaluation happens
  while writing restObj to the target: PutValue re-evaluates the target's
  base expression first and only roots restObj once that base value is in
  hand, so restObj must still survive a collection triggered by the base
  expression itself (e.g. a function call).
features: [host-gc-required, object-rest, destructuring-assignment]
---*/

function churnAndGc() {
  $262.gc();
  var churn = [];
  for (var i = 0; i < 500; i++) {
    churn.push({ churn: i });
  }
}

var target = {};
var calls = 0;
function getTarget() {
  calls++;
  churnAndGc();
  return target;
}

({ ...getTarget().k } = { a: 1, b: 2 });

assert.sameValue(calls, 1, "the target base expression ran exactly once");
assert.sameValue(target.k.a, 1, "the rest object's own property 'a' survives the base expression's collection");
assert.sameValue(target.k.b, 2, "the rest object's own property 'b' survives the base expression's collection");
assert.sameValue(
  Object.getPrototypeOf(target.k),
  Object.prototype,
  "the rest object itself (not a reused, freed slot) is the one assigned"
);
assert.sameValue(
  Object.getOwnPropertyNames(target.k).length,
  2,
  "the rest object has exactly its two copied own properties"
);
