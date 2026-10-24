/*---
description: >
  JSON.parse keeps the holder and the values it is walking reachable while the
  reviver runs, across forced collections.
esid: sec-json.parse
info: |
  InternalizeJSONProperty gets val from holder, recurses into its elements and
  properties, then calls the reviver with holder as the this value. The
  intermediate holders and values are owned only by the algorithm while a
  reviver call (which may collect) is in progress.

  Regression test for issue #1198: the internalize walk did not root its
  holder or the walked value, so a collection triggered inside the reviver
  reclaimed parsed objects and the result read back as undefined.
features: [host-gc-required]
---*/

var result = JSON.parse('{"a":{"b":1,"c":[2,{"d":3}]},"e":[[4],[5]]}', function (k, v) {
  $262.gc();
  return v;
});

assert.sameValue(typeof result.a, "object", "result.a");
assert.sameValue(result.a.b, 1, "result.a.b");
assert.sameValue(result.a.c[0], 2, "result.a.c[0]");
assert.sameValue(result.a.c[1].d, 3, "result.a.c[1].d");
assert.sameValue(result.e[0][0], 4, "result.e[0][0]");
assert.sameValue(result.e[1][0], 5, "result.e[1][0]");

var replaced = JSON.parse('{"a":[1,2],"b":{"c":3}}', function (k, v) {
  $262.gc();
  return typeof v === "number" ? { n: v } : v;
});

assert.sameValue(replaced.a[0].n, 1, "replaced.a[0].n");
assert.sameValue(replaced.a[1].n, 2, "replaced.a[1].n");
assert.sameValue(replaced.b.c.n, 3, "replaced.b.c.n");

var deleted = JSON.parse('{"a":{"x":1},"b":{"y":2}}', function (k, v) {
  if (k === "a") {
    delete this.b;
  }
  $262.gc();
  return v;
});

assert.sameValue(deleted.a.x, 1, "deleted.a.x");
assert.sameValue(Object.prototype.hasOwnProperty.call(deleted, "b"), false, "deleted.b");
