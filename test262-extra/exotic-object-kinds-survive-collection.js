/*---
description: >
  Arrays (with extra named properties and holes), RegExps, iterator helpers,
  WeakRefs, bound functions and objects of every other exotic kind keep their
  internal state across collections that free and reuse the objects around them.
esid: sec-array-exotic-objects
info: |
  Array exotic objects, RegExp instances, Iterator Helper objects, WeakRef
  instances and bound function exotic objects each carry kind-specific
  internal slots. Those slots live outside the object's inline storage; the
  collector must trace them and a collection must not disturb them. Every
  object here is reachable only through the array that holds it.
features: [WeakRef, Symbol.iterator, iterator-helpers, host-gc-required]
includes: [compareArray.js]
---*/

function make(i) {
  var arr = [i, i + 1, , i + 3];
  arr.label = "array" + i;
  arr[Symbol.for("k" + i)] = i;
  var re = new RegExp("a(" + i + ")b", "g");
  var it = [i, i + 1, i + 2].values().map(function (x) {
    return x * 2;
  });
  var target = { n: i };
  var ref = new WeakRef(target);
  var bound = function (a, b) {
    return this.base + a + b;
  }.bind({ base: i }, 1);
  return { arr: arr, re: re, it: it, target: target, ref: ref, bound: bound };
}

var kept = [];
for (var i = 0; i < 30; i++) {
  kept.push(make(i));
}

for (var round = 0; round < 3; round++) {
  var junk = [];
  for (var j = 0; j < 2000; j++) {
    junk.push([j, j + 1, j + 2]);
    junk.push(/x/);
  }
  junk = undefined;
  $262.gc();
}

for (var i = 0; i < 30; i++) {
  var o = kept[i];
  assert.sameValue(o.arr.length, 4, "length " + i);
  assert.sameValue(o.arr[0], i);
  assert.sameValue(1 in o.arr, true);
  assert.sameValue(2 in o.arr, false, "hole stays a hole");
  assert.sameValue(o.arr[3], i + 3);
  assert.sameValue(o.arr.label, "array" + i);
  assert.sameValue(o.arr[Symbol.for("k" + i)], i);
  assert.compareArray(Object.keys(o.arr), ["0", "1", "3", "label"]);
  assert.sameValue(o.re.source, "a(" + i + ")b");
  assert.sameValue(o.re.flags, "g");
  assert.sameValue(o.re.test("xa" + i + "by"), true);
  assert.compareArray(Array.from(o.it), [i * 2, (i + 1) * 2, (i + 2) * 2]);
  assert.sameValue(o.ref.deref(), o.target, "WeakRef target kept alive by a strong reference");
  assert.sameValue(o.bound(2), i + 3);
  assert.sameValue(o.bound.name, "bound ");
}
