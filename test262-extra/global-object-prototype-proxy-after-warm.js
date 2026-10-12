/*---
description: >
  A warmed own global data property never invokes Proxy traps on the global
  object's prototype; once the property is deleted, bare read, write and
  typeof reach the has/get/set traps.
info: |
  Object Environment Record HasBinding (sec-object-environment-records-hasbinding-n)
  calls HasProperty, which walks the prototype chain (sec-ordinaryhasproperty);
  an own property short-circuits before any prototype Proxy trap.
esid: sec-object-environment-records-hasbinding-n
flags: [noStrict]
---*/

globalThis.px = 1;
function rd() { return px; }
function wr(v) { px = v; }
function ty() { return typeof px; }
for (var i = 0; i < 10; i++) { wr(i); rd(); ty(); }

var log = [];
var origProto = Object.getPrototypeOf(globalThis);
var proxy = new Proxy(origProto, {
  has: function (t, k) { if (k === "px") log.push("has"); return Reflect.has(t, k); },
  get: function (t, k, r) { if (k === "px") log.push("get"); return Reflect.get(t, k, r); },
  set: function (t, k, v, r) { if (k === "px") log.push("set"); return Reflect.set(t, k, v, r); },
});
Object.setPrototypeOf(globalThis, proxy);
try {
  rd(); wr(3); ty();
  assert.sameValue(log.length, 0, "own data property: no trap fires");
  assert.sameValue(globalThis.px, 3);

  delete globalThis.px;
  log.length = 0;
  assert.throws(ReferenceError, rd);
  assert.sameValue(log.join(), "has", "read after delete: has trap only");
  log.length = 0;
  assert.sameValue(ty(), "undefined");
  assert.sameValue(log.join(), "has", "typeof after delete: has trap only");
} finally {
  Object.setPrototypeOf(globalThis, origProto);
}
