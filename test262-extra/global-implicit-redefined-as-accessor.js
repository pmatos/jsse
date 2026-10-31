/*---
description: >
  An implicit global redefined as an accessor property is written through its
  setter and read through its getter; the property keeps no stale value.
info: |
  PutValue (sec-putvalue) on an unresolvable reference performs
  Set(globalObj, name, V, false); a resolvable reference to a global property
  reaches Object Environment Record SetMutableBinding
  (sec-object-environment-records-setmutablebinding-n-v-s) and GetBindingValue
  (sec-object-environment-records-getbindingvalue-n-s), which call [[Set]]
  and [[Get]] on the global object.
esid: sec-object-environment-records-setmutablebinding-n-v-s
flags: [noStrict]
---*/

var log = [];
x1346 = 1;
Object.defineProperty(globalThis, "x1346", {
  get: function () { log.push("get"); return 9; },
  set: function (v) { log.push("set:" + v); },
  configurable: true
});

x1346 = 2;
assert.sameValue(log.join(), "set:2", "sloppy write calls the setter");
assert.sameValue(x1346, 9, "read calls the getter");
assert.sameValue(log.join(), "set:2,get");

var desc = Object.getOwnPropertyDescriptor(globalThis, "x1346");
assert.sameValue(typeof desc.get, "function");
assert.sameValue(typeof desc.set, "function");
assert.sameValue("value" in desc, false, "no value on an accessor descriptor");
assert.sameValue("writable" in desc, false, "no writable on an accessor descriptor");

log.length = 0;
(function () { "use strict"; x1346 = 3; })();
assert.sameValue(log.join(), "set:3", "strict write calls the setter");

log.length = 0;
x1346 += 1;
assert.sameValue(log.join(), "get,set:10", "compound assignment reads then writes");

assert.sameValue(typeof x1346, "number", "typeof reads through the getter");
