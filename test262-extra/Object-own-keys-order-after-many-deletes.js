/*---
description: >
  [[OwnPropertyKeys]] reports String keys in property creation order, after
  integer indices and before Symbols, for objects with many properties that
  have had properties deleted and re-added.
esid: sec-ordinaryownpropertykeys
includes: [compareArray.js]
features: [Symbol, Reflect]
info: |
  OrdinaryOwnPropertyKeys: integer indices in ascending order, then String
  keys in ascending property-creation order, then Symbol keys in ascending
  property-creation order. Deleting a property and creating it again makes it
  the newest key; redefining an existing property keeps its position.

  Regression test for issue #911: the engine's property storage keeps creation
  order itself, with deleted entries compacted away once they pile up, and
  must report exactly this order whatever the object's size or history.
---*/

var sym1 = Symbol("one");
var sym2 = Symbol("two");

function names(n) {
  var out = [];
  for (var i = 0; i < n; i++) out.push("k" + i);
  return out;
}

[3, 8, 9, 40, 300].forEach(function (n) {
  var obj = {};
  obj[sym1] = 1;
  var expected = names(n);
  expected.forEach(function (k, i) {
    obj[k] = i;
  });
  obj[sym2] = 2;

  // Delete a scattered subset, enough of them that a large object compacts
  // its storage more than once.
  var kept = [];
  expected.forEach(function (k, i) {
    if (i % 3 === 0 || i % 7 === 0) {
      assert.sameValue(delete obj[k], true, "delete " + k);
    } else {
      kept.push(k);
    }
  });
  assert.compareArray(Object.keys(obj), kept, "keys after deletes, n=" + n);

  // A redefined property keeps its place; a re-added one moves to the end.
  if (kept.length > 1) {
    Object.defineProperty(obj, kept[0], { value: "redefined" });
    obj[kept[1]] = "assigned";
  }
  obj["k0"] = "readded";
  obj[5] = "five";
  obj[2] = "two";
  obj["10"] = "ten";

  var expectedKeys = ["2", "5", "10"].concat(kept);
  if (kept.indexOf("k0") === -1) expectedKeys.push("k0");
  assert.compareArray(Reflect.ownKeys(obj), expectedKeys.concat([sym1, sym2]), "ownKeys, n=" + n);
  assert.compareArray(Object.getOwnPropertyNames(obj), expectedKeys, "names, n=" + n);
  assert.compareArray(Object.getOwnPropertySymbols(obj), [sym1, sym2], "symbols, n=" + n);

  var forIn = [];
  for (var key in obj) forIn.push(key);
  assert.compareArray(forIn, expectedKeys, "for-in, n=" + n);

  var json = JSON.stringify(obj);
  var fromJson = Object.keys(JSON.parse(json));
  assert.compareArray(fromJson, expectedKeys, "JSON round trip, n=" + n);

  assert.compareArray(Object.entries(obj).map(function (e) { return e[0]; }), expectedKeys, "entries, n=" + n);
  assert.compareArray(Object.keys(Object.assign({}, obj)), expectedKeys, "assign, n=" + n);
  assert.compareArray(Object.keys(Object.fromEntries(Object.entries(obj))), expectedKeys, "fromEntries, n=" + n);
});

// Deleting everything and starting over leaves a usable, correctly ordered object.
var drained = {};
for (var i = 0; i < 64; i++) drained["a" + i] = i;
for (var i = 0; i < 64; i++) delete drained["a" + i];
assert.compareArray(Reflect.ownKeys(drained), [], "drained");
drained.z = 1;
drained.y = 2;
assert.compareArray(Object.keys(drained), ["z", "y"], "reused after draining");
