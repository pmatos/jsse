/*---
description: >
  The same global var name in two realms resolves per realm: a function from
  realm B reads and writes B's global even when called from realm A, and hot
  loops in each realm do not cross over.
info: |
  Each Realm Record owns its global object and Global Environment Record
  (sec-code-realms); a function's scope chain ends at its own realm's
  global environment.
esid: sec-code-realms
flags: [noStrict]
---*/

var shared = "A";
var other = $262.createRealm();
other.evalScript("var shared = 'B'; function readB() { return shared; } function writeB(v) { shared = v; }");
function readA() { return shared; }
function writeA(v) { shared = v; }

for (var i = 0; i < 20; i++) {
  assert.sameValue(readA(), "A");
  assert.sameValue(other.global.readB(), "B");
}
other.global.writeB("B2");
assert.sameValue(readA(), "A", "A unchanged by B write");
assert.sameValue(other.global.shared, "B2");
writeA("A2");
assert.sameValue(other.global.readB(), "B2", "B unchanged by A write");
assert.sameValue(globalThis.shared, "A2");

var readB = other.global.readB;
for (var j = 0; j < 20; j++) { writeA("x" + j); assert.sameValue(readB(), "B2"); }

assert.sameValue(other.evalScript("var cnt = 0; for (var q = 0; q < 50; q++) { cnt = cnt + q; } cnt"), 1225);
assert.sameValue(typeof cnt, "undefined", "other realm's var is not visible here");
