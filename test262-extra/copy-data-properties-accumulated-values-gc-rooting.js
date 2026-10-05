// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-copydataproperties
description: >
  CopyDataProperties fetches each own enumerable property's value before any
  of them are written into the target object. A value already fetched for
  an earlier key must stay reachable across the Get() of a later key, even
  though that value exists only in CopyDataProperties's own internal
  accumulator and has no reference from any JavaScript-visible value yet.
  Exercised through both of CopyDataProperties's callers: object-rest
  destructuring and object-literal spread.
info: |
  CopyDataProperties ( target, source, excludedItems )

  3. If source is not undefined or null, then
    ...
    c. For each element nextKey of keys, do
      i. Let desc be ? from.[[GetOwnProperty]](nextKey).
      ii. If desc is not undefined and desc.[[Enumerable]] is true, then
        1. Let propValue be ? Get(from, nextKey).
        2. Perform ! CreateDataPropertyOrThrow(target, nextKey, propValue).

  Each Get(from, nextKey) can invoke a getter that runs arbitrary code
  (including a garbage collection) while an earlier propValue has not yet
  been written to target.
features: [host-gc-required, object-rest, destructuring-binding]
---*/

function churnAndGc() {
  $262.gc();
  var churn = [];
  for (var i = 0; i < 500; i++) {
    churn.push({ churn: i });
  }
}

// Object-rest destructuring path (bind_object_rest_values -> copy_data_properties).
var source1 = {
  get a() {
    return { tag: "first" };
  },
  get b() {
    churnAndGc();
    return 1;
  },
};

const { ...x } = source1;

assert.sameValue(x.a.tag, "first", "the first key's object value survives the second key's getter");
assert.sameValue(
  Object.getOwnPropertyNames(x.a).length,
  1,
  "the first key's value is exactly the object the getter returned, not a reused slot"
);
assert.sameValue(
  "churn" in x.a,
  false,
  "the first key's value was not overwritten by the churn allocator reusing its freed arena slot"
);

// Object-literal spread path (eval_object_literal -> copy_data_properties).
var source2 = {
  get c() {
    return { tag: "second" };
  },
  get d() {
    churnAndGc();
    return 2;
  },
};

const y = { ...source2 };

assert.sameValue(y.c.tag, "second", "the first key's object value survives the second key's getter");
assert.sameValue(
  Object.getOwnPropertyNames(y.c).length,
  1,
  "the first key's value is exactly the object the getter returned, not a reused slot"
);
assert.sameValue(
  "churn" in y.c,
  false,
  "the first key's value was not overwritten by the churn allocator reusing its freed arena slot"
);
