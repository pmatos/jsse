/*---
description: >
  Installing an accessor over a warmed global data property routes later bare
  reads and writes through the getter and setter exactly once per operation,
  with the global object as receiver; `x++` calls the getter then the setter.
info: |
  OrdinaryGet (sec-ordinaryget) and OrdinarySetWithOwnDescriptor
  (sec-ordinarysetwithowndescriptor) call accessor functions with the receiver.
esid: sec-ordinaryget
flags: [noStrict]
---*/

globalThis.ac = 1;
function rd() { return ac; }
function wr(v) { ac = v; }
function inc() { ac++; }
for (var i = 0; i < 10; i++) { wr(i); rd(); inc(); }

var log = [];
var stored = 5;
Object.defineProperty(globalThis, "ac", {
  configurable: true,
  get: function () { log.push(["get", this === globalThis]); return stored; },
  set: function (v) { log.push(["set", this === globalThis, v]); stored = v; },
});

assert.sameValue(rd(), 5);
assert.sameValue(log.length, 1, "one getter call per read");
assert.sameValue(log[0][1], true, "getter receiver is the global object");

log.length = 0;
wr(9);
assert.sameValue(log.length, 1, "one setter call per write");
assert.sameValue(log[0][0], "set");
assert.sameValue(log[0][1], true);
assert.sameValue(log[0][2], 9);

log.length = 0;
inc();
assert.sameValue(log.length, 2, "x++ is one get and one set");
assert.sameValue(log[0][0], "get");
assert.sameValue(log[1][0], "set");
assert.sameValue(log[1][2], 10);
assert.sameValue(stored, 10);

log.length = 0;
assert.sameValue(typeof ac, "number");
assert.sameValue(log.length, 1, "typeof calls the getter once");
