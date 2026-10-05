// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-destructuring-binding-patterns-runtime-semantics-restbindinginitialization
description: >
  The fresh rest object built by RestBindingInitialization
  (`var {...x} = source`) stays reachable while it is handed off to the
  BindingIdentifier's own binding step, even when that step runs arbitrary
  user code (a `with` statement's @@unscopables getter) before the rest
  object becomes reachable from the environment.
info: |
  RestBindingInitialization
  BindingRestProperty : ... BindingIdentifier

  1. Let lhs be ? ResolveBinding(StringValue of BindingIdentifier, environment).
  2. Let restObj be OrdinaryObjectCreate(%Object.prototype%).
  3. Perform ? CopyDataProperties(restObj, value, excludedNames).
  4. If environment is undefined, return ? PutValue(lhs, restObj).
  5. Return ? InitializeReferencedBinding(lhs, restObj).

  Step 1's BindingIdentifier resolution, when the running code is inside a
  `with` statement, calls HasBinding on the with object's environment
  record, which invokes @@unscopables — arbitrary code that can run a
  garbage collection. This test does not assert the relative order of that
  call and CopyDataProperties's own Get(source, key) (also arbitrary code
  that can run a garbage collection) — only that restObj, once built,
  survives either one: it has no JavaScript-visible reference to it until
  the BindingIdentifier step completes.
features: [host-gc-required, object-rest, destructuring-binding]
flags: [noStrict]
---*/

var o = {
  x: 0,
  get [Symbol.unscopables]() {
    $262.gc();
    var churn = [];
    for (var i = 0; i < 500; i++) {
      churn.push({ churn: i });
    }
    return {};
  },
};

with (o) {
  var { ...x } = {
    get a() {
      return { tag: "first" };
    },
  };
}

assert.sameValue(
  Object.getPrototypeOf(o.x),
  Object.prototype,
  "the rest object itself (not a reused, freed slot) is the one bound"
);
assert.sameValue(
  Object.getOwnPropertyNames(o.x).join(),
  "a",
  "the rest object's only own property is the copied one, not a reused churn slot"
);
assert.sameValue(o.x.a.tag, "first", "the rest object's own property survives the @@unscopables getter's collection");
