/*---
description: >
  A global property that becomes non-writable after its binding was warmed is
  not written: sloppy mode ignores the assignment, strict mode throws.
info: |
  Object Environment Record SetMutableBinding (sec-object-environment-records-setmutablebinding-n-v-s)
  calls [[Set]] and throws TypeError when it returns false and S is true;
  OrdinarySetWithOwnDescriptor (sec-ordinarysetwithowndescriptor) returns
  false for a non-writable data property.
esid: sec-object-environment-records-setmutablebinding-n-v-s
flags: [noStrict]
---*/

globalThis.nw = 1;
function sloppyWrite(v) { nw = v; }
function strictWrite(v) { "use strict"; nw = v; }
for (var i = 0; i < 10; i++) { sloppyWrite(i); strictWrite(i); }
assert.sameValue(nw, 9);

Object.defineProperty(globalThis, "nw", { writable: false });
sloppyWrite(100);
assert.sameValue(nw, 9, "sloppy write ignored");
assert.throws(TypeError, function () { strictWrite(100); }, "strict write throws");
assert.sameValue(nw, 9, "value unchanged");
assert.sameValue(Object.getOwnPropertyDescriptor(globalThis, "nw").value, 9);

Object.defineProperty(globalThis, "nw", { writable: true });
sloppyWrite(7);
assert.sameValue(nw, 7, "writable again");

globalThis.fz = 1;
function writeFz(v) { fz = v; }
function readFz() { return fz; }
for (var j = 0; j < 5; j++) { writeFz(j); readFz(); }
var frozenGlobal = { nw: 0 };
Object.defineProperty(globalThis, "fz", { writable: false, configurable: false });
writeFz(50);
assert.sameValue(readFz(), 4, "non-writable non-configurable stays");
