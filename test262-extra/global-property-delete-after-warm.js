/*---
description: >
  Deleting a configurable global property after its bare-name reads and writes
  were warmed makes later reads throw ReferenceError, strict writes throw
  ReferenceError, sloppy writes recreate it, and typeof yield "undefined".
info: |
  Object Environment Record HasBinding (sec-object-environment-records-hasbinding-n),
  GetBindingValue (...-getbindingvalue-n-s) and SetMutableBinding
  (...-setmutablebinding-n-v-s) re-check HasProperty on every access.
esid: sec-object-environment-records-getbindingvalue-n-s
flags: [noStrict]
---*/

globalThis.dp = 1;
function rd() { return dp; }
function wr(v) { dp = v; }
function swr(v) { "use strict"; dp = v; }
function ty() { return typeof dp; }
for (var i = 0; i < 10; i++) { wr(i); rd(); ty(); swr(i); }

assert.sameValue(delete globalThis.dp, true);
assert.throws(ReferenceError, rd, "read after delete");
assert.sameValue(ty(), "undefined", "typeof after delete");
assert.throws(ReferenceError, function () { swr(1); }, "strict write after delete");
assert.sameValue(Object.prototype.hasOwnProperty.call(globalThis, "dp"), false);
wr(42);
assert.sameValue(rd(), 42, "sloppy write recreates");
assert.sameValue(globalThis.dp, 42);
assert.sameValue(ty(), "number");
