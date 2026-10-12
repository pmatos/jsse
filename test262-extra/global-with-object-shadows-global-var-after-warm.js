/*---
description: >
  A with-statement object shadows a warmed global var for bare reads and
  writes, and Symbol.unscopables hides the name again.
info: |
  Object Environment Record HasBinding with withEnvironment true
  (sec-object-environment-records-hasbinding-n) consults @@unscopables.
esid: sec-with-statement
flags: [noStrict]
---*/

var wx = "g";
function rd() { return wx; }
for (var i = 0; i < 10; i++) { rd(); wx = "g" + i; }

var o = { wx: "w" };
with (o) {
  for (var i = 0; i < 5; i++) {
    assert.sameValue(wx, "w");
    wx = "w" + i;
    assert.sameValue(o.wx, "w" + i);
    wx = "w";
  }
}
assert.sameValue(o.wx, "w");
assert.sameValue(wx, "g9", "global var untouched");

var u = { wx: "hidden", [Symbol.unscopables]: { wx: true } };
with (u) {
  assert.sameValue(wx, "g9", "unscopable name resolves outside");
  wx = "g-new";
}
assert.sameValue(u.wx, "hidden");
assert.sameValue(rd(), "g-new");
