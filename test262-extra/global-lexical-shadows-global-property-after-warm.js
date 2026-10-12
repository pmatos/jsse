/*---
description: >
  A global lexical declaration added by a later script shadows a warmed global
  object property: bare reads and writes then resolve to the declarative
  record, the global object property is untouched, const assignment throws,
  and a TDZ read from a warmed function throws ReferenceError.
info: |
  Global Environment Record HasBinding (sec-global-environment-records-hasbinding-n)
  consults the declarative record before the object record; GlobalDeclarationInstantiation
  (sec-globaldeclarationinstantiation) allows a lexical declaration over a
  configurable global property.
esid: sec-global-environment-records-hasbinding-n
flags: [noStrict]
---*/

globalThis.zz = 1;
function rd() { return zz; }
function wr(v) { zz = v; }
for (var i = 0; i < 10; i++) { wr(i); rd(); }
assert.sameValue(rd(), 9);

$262.evalScript("let zz = 5;");
assert.sameValue(rd(), 5, "read hits lexical binding");
wr(6);
assert.sameValue(rd(), 6, "write hits lexical binding");
assert.sameValue(globalThis.zz, 9, "global object property untouched");

globalThis.cc = 1;
function rdc() { return cc; }
function wrc(v) { cc = v; }
for (var j = 0; j < 10; j++) { wrc(j); rdc(); }
$262.evalScript("const cc = 3;");
assert.sameValue(rdc(), 3);
assert.throws(TypeError, function () { wrc(4); }, "const assignment");
assert.sameValue(globalThis.cc, 9);

globalThis.tdz = 1;
function rdt() { return tdz; }
for (var k = 0; k < 10; k++) { rdt(); }
assert.throws(ReferenceError, function () {
  $262.evalScript("rdt(); let tdz = 1;");
}, "TDZ read from warmed function");
