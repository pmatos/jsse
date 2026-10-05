// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-restdestructuringassignmentevaluation
description: >
  The assignment target's base expression for an object assignment pattern's
  rest property (`({...base().k} = source)`) is evaluated before
  CopyDataProperties runs on the source, per AssignmentRestProperty's
  evaluation order. The fresh rest object this produces stays reachable
  across the garbage collection that can happen while CopyDataProperties
  invokes a getter on the source, even though the rest object has no
  JavaScript-visible reference to it yet.
info: |
  AssignmentRestProperty : ... DestructuringAssignmentTarget

  1. Let lRef be ? Evaluation of DestructuringAssignmentTarget.
  2. Let restObj be OrdinaryObjectCreate(%Object.prototype%).
  3. Perform ? CopyDataProperties(restObj, value, excludedNames).
  4. Return ? PutValue(lRef, restObj).

  Step 1 (evaluating the target's base expression) must run before step 3,
  which can invoke a getter on the source that runs arbitrary code (including
  a garbage collection) while restObj is not yet reachable from any
  JavaScript-visible value.
features: [host-gc-required, object-rest, destructuring-assignment]
---*/

var log = [];
var target = {};
var calls = 0;
function getTarget() {
  calls++;
  log.push("base");
  return target;
}

function churnAndGc() {
  log.push("get");
  $262.gc();
  var churn = [];
  for (var i = 0; i < 500; i++) {
    churn.push({ churn: i });
  }
}

var source = {
  a: 1,
  get b() {
    churnAndGc();
    return 2;
  },
};

({ ...getTarget().k } = source);

assert.sameValue(calls, 1, "the target base expression ran exactly once");
assert.sameValue(
  log.join(","),
  "base,get",
  "the target's base expression evaluates before CopyDataProperties's Get on the source"
);
assert.sameValue(target.k.a, 1, "the rest object's own property 'a' survives the source getter's collection");
assert.sameValue(target.k.b, 2, "the rest object's own property 'b' survives the source getter's collection");
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
