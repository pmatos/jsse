// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-destructuring-binding-patterns-runtime-semantics-restbindinginitialization
description: >
  The fresh rest object created by RestBindingInitialization
  (`const {...x} = source`) stays reachable across the garbage collection
  that can happen while CopyDataProperties invokes a getter on the source,
  even though the rest object has no JavaScript-visible reference to it yet.
info: |
  RestBindingInitialization
  BindingRestProperty : ... BindingIdentifier

  1. Let restObj be OrdinaryObjectCreate(%Object.prototype%).
  2. Perform ? CopyDataProperties(restObj, value, excludedNames).
  3. Return ? BindingInitialization of BindingIdentifier with arguments
     restObj and environment.

  restObj is created before CopyDataProperties runs, and CopyDataProperties's
  own Get(source, key) step can invoke a getter that runs arbitrary code
  (including a garbage collection) while restObj is not yet reachable from
  any JavaScript-visible value.
features: [host-gc-required, object-rest, destructuring-binding]
---*/

function churnAndGc() {
  $262.gc();
  var churn = [];
  for (var i = 0; i < 500; i++) {
    churn.push({ churn: i });
  }
}

const { ...x } = {
  get v() {
    churnAndGc();
    return 2;
  },
};

assert.sameValue(x.v, 2, "the rest object's own property survives the getter's collection");
assert.sameValue(
  Object.getPrototypeOf(x),
  Object.prototype,
  "the rest object itself (not a reused, freed slot) is the one returned"
);
assert.sameValue(
  Object.getOwnPropertyNames(x).length,
  1,
  "the rest object has exactly one own property"
);
assert.sameValue(
  Object.getOwnPropertyNames(x)[0],
  "v",
  "the rest object's only own property is the copied one"
);
