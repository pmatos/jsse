// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-destructuring-binding-patterns-runtime-semantics-keyedbindinginitialization
description: >
  The ToObject-wrapped source value for an object binding pattern
  (`var {...} = source`) stays reachable across the whole pattern, not just
  the one property access it was created for, even though it has no
  JavaScript-visible reference to it until every property has been bound.
info: |
  ObjectBindingPattern : { BindingPropertyList }

  BindingInitialization of ObjectBindingPattern coerces value to an object
  once (PropertyBindingInitialization's own RequireObjectCoercible/ToObject
  step), then walks every BindingProperty in source order, performing a
  GetV/computed-key-evaluation/CopyDataProperties for each. A computed
  property key's own evaluation can run arbitrary code (including a garbage
  collection) while the coerced source object has no reference from any
  JavaScript-visible value — it exists only for the duration of this single
  binding pattern.
features: [host-gc-required, object-rest, destructuring-binding, computed-property-names]
---*/

function churnAndGc() {
  $262.gc();
  var churn = [];
  for (var i = 0; i < 500; i++) {
    churn.push({ churn: i });
  }
  return "a";
}

var { [churnAndGc()]: a1, ...x } = { a: 1, b: { deep: "keep" } };

assert.sameValue(a1, 1, "the computed-key property is bound correctly");
assert.sameValue(
  Object.getOwnPropertyNames(x).join(),
  "b",
  "the rest object's only own property is the copied one, not a reused churn slot"
);
assert.sameValue(x.b.deep, "keep", "the rest object's own property value survives the computed key's collection");

// Same hazard without a rest property: the source value must still outlive
// every KeyValue/Shorthand property access in the pattern, not just the one
// that triggered the collection.
var { [churnAndGc()]: a2, b: b2 } = { a: 2, b: 3 };
assert.sameValue(a2, 2, "the computed-key property is bound correctly (no rest)");
assert.sameValue(b2, 3, "a later property survives the computed key's collection (no rest)");
