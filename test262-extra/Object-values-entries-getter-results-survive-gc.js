// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-enumerableownproperties
description: >
  Values and entry arrays already collected by Object.values / Object.entries
  stay reachable while a later getter runs (and collects garbage), for an
  ordinary object and for a String wrapper, whose exotic keys take the general
  [[OwnPropertyKeys]] path.
info: |
  7.3.23 EnumerableOwnProperties ( O, kind )
    3. For each element key of ownKeys, do
      ...
        2. Else, let value be ? Get(O, key).
        3. If kind is value, append value to properties.
        4. Else, let entry be CreateArrayFromList(« key, value »). Append entry to properties.
features: [host-gc-required]
includes: [compareArray.js]
---*/

function collect() {
  $262.gc();
  var churn = [];
  for (var i = 0; i < 500; i++) {
    churn.push({ churn: i });
  }
}

function build(target) {
  for (var i = 0; i < 3; i++) {
    (function (n) {
      Object.defineProperty(target, "g" + n, {
        enumerable: true,
        get: function () {
          collect();
          return { id: n };
        }
      });
    })(i);
  }
  return target;
}

[{}, new String("ab")].forEach(function (target) {
  var o = build(target);
  var values = Object.values(o).filter(function (v) { return typeof v === "object"; });
  assert.compareArray(values.map(function (v) { return v.id; }), [0, 1, 2], "values");

  var entries = Object.entries(o).filter(function (e) { return typeof e[1] === "object"; });
  assert.compareArray(
    entries.map(function (e) { return e[0] + ":" + e[1].id; }),
    ["g0:0", "g1:1", "g2:2"],
    "entries"
  );
});
