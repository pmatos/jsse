/*---
description: >
  A script-level var and the same-named global object property are one
  binding: bare reads and writes observe writes made through globalThis and
  vice versa, in a hot loop at top level and inside a function.
info: |
  Global Environment Records (sec-global-environment-records) delegate
  GetBindingValue / SetMutableBinding to the Object Environment Record whose
  binding object is the global object.
esid: sec-global-environment-records
flags: [noStrict]
---*/

var gv = 0;
function bump() { gv = gv + 1; return gv; }

for (var i = 0; i < 20; i++) {
  gv = gv + 1;
  assert.sameValue(globalThis.gv, i + 1, "bare write visible via globalThis");
  globalThis.gv = globalThis.gv + 1;
  assert.sameValue(gv, i + 2, "globalThis write visible via bare read");
  gv = i + 1;
}
assert.sameValue(bump(), 21, "function reads/writes the same binding");
gv++;
assert.sameValue(globalThis.gv, 22, "postfix increment writes the property");
gv += 4;
assert.sameValue(globalThis.gv, 26, "compound assignment writes the property");
assert.sameValue(typeof gv, "number", "typeof reads the property");
globalThis.gv = "s";
assert.sameValue(typeof gv, "string", "typeof after write via globalThis");
assert.sameValue(Object.getOwnPropertyDescriptor(globalThis, "gv").configurable, false, "var is non-configurable");
