/*---
description: >
  A proxy trap that runs while the proxy is only reachable from the caller's
  temporary keeps the proxy and its target alive, so the invariant checks that
  follow the trap still see the target.
esid: sec-proxy-object-internal-methods-and-internal-slots-delete-p
info: |
  [[Delete]] (P), step 14: If targetDesc.[[Configurable]] is false, throw a
  TypeError exception. The check reads the target after the trap returned.

  Regression test: invoke_proxy_trap kept neither the proxy nor the arguments
  it passed on the collector's roots, so a collection during the trap
  reclaimed the target of a proxy that was a temporary of the delete
  expression and the invariant check found no property to compare.
features: [Proxy, host-gc-required]
---*/

function frozenX() {
  return Object.freeze({ x: 1 });
}

for (var i = 0; i < 20; i++) {
  assert.throws(TypeError, function () {
    delete new Proxy(frozenX(), { deleteProperty: function () { return true; } }).x;
  }, "deleteProperty trap result true for a non-configurable property");

  assert.throws(TypeError, function () {
    "x" in new Proxy(frozenX(), { has: function () { return false; } });
  }, "has trap result false for a non-configurable property");

  assert.throws(TypeError, function () {
    Object.getOwnPropertyDescriptor(
      new Proxy(frozenX(), { getOwnPropertyDescriptor: function () { return undefined; } }),
      "x"
    );
  }, "getOwnPropertyDescriptor trap result undefined for a non-configurable property");

  assert.throws(TypeError, function () {
    new Proxy(frozenX(), { set: function () { return true; } }).x = 2;
  }, "set trap result true for a non-writable, non-configurable property");
}
