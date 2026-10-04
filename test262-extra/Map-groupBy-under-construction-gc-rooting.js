// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-map.groupby
description: >
  The iterator record obtained from the items argument, and the result Map
  under construction, stay reachable across the garbage collections that
  can happen while the callback runs arbitrary user code partway through
  the GroupBy loop.
info: |
  Map.groupBy ( items, callback )

  1. Let groups be ? GroupBy(items, callback, collection).
  2. Let map be ! Construct(%Map%).
  ...

  GroupBy ( items, callback, keyCoercion )

  4. Let iteratorRecord be ? GetIterator(items, sync).
  5. Let k be 0.
  6. Repeat,
    a. Let next be ? IteratorStepValue(iteratorRecord).
    b. If next is done, return groups.
    c. Let value be next.
    d. Let key be Completion(Call(callback, undefined, « value, 𝔽(k) »)).
    e. IfAbruptCloseIterator(key, iteratorRecord).
    ...
    g. Perform AddValueToKeyedGroup(groups, key, value).
    h. Set k to k + 1.

  `iteratorRecord` and the in-progress `groups`/result map must stay the
  same object across every step above; the callback call in step d can run
  arbitrary code (including triggering a collection) before the loop reads
  the iterator's `next` method again on its following iteration.
features: [host-gc-required]
---*/

function collect() {
  $262.gc();
  var churn = [];
  for (var i = 0; i < 500; i++) {
    churn.push({ churn: i });
  }
}

var result = Map.groupBy([0, 1, 2, 3], function (value) {
  collect();
  return value % 2 === 0 ? "even" : "odd";
});

assert.sameValue(result.size, 2);
assert.compareArray(result.get("even"), [0, 2]);
assert.compareArray(result.get("odd"), [1, 3]);
