/*---
description: >
  Assigning freshly allocated objects, closures and strings to a warmed global
  var across forced collections keeps the latest value reachable and intact.
info: |
  Pure implementation-behaviour regression for the global write path: the
  global object is long-lived while the stored value is young, so the store
  must record the old-to-young edge (generational GC write barrier).
esid: sec-putvalue
flags: [noStrict]
includes: [compareArray.js]
---*/

var slot = null;
var keep = [];
for (var i = 0; i < 200; i++) {
  slot = { n: i, arr: [i, i + 1], f: function () { return i; } };
  if (i % 17 === 0) { $262.gc(); }
  if (i % 40 === 0) { keep.push({ filler: i }); }
}
$262.gc();
assert.sameValue(slot.n, 199);
assert.compareArray(slot.arr, [199, 200]);
slot = "str" + "ing" + 1;
$262.gc();
assert.sameValue(slot, "string1");
slot = function () { return 7; };
$262.gc();
assert.sameValue(slot(), 7);
var g2 = { deep: { deeper: [1, 2, 3] } };
$262.gc();
assert.compareArray(g2.deep.deeper, [1, 2, 3]);
