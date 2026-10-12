/*---
description: >
  Map and Set keep insertion order, SameValueZero lookup and live iteration
  exact when many entries are added and deleted, including deletions heavy
  enough to reclaim the storage of deleted entries while an iterator, forEach
  or Set method is suspended.
esid: sec-map.prototype.delete
info: |
  Map.prototype.delete (24.1.3.3) and Set.prototype.delete (24.2.4.5) set the
  element to ~empty~ and leave the entry list in place, so an iterator's index
  keeps naming the same element no matter how many other elements are deleted,
  and elements appended later are still visited. Map.prototype.set
  (24.1.3.9) and Set.prototype.add (24.2.4.1) look keys up with SameValueZero
  and append new elements at the end, a deleted key re-added going last.
includes: [compareArray.js]
features: [Symbol, BigInt]
---*/

function keysOf(collection) {
  return Array.from(collection.keys());
}

var N = 300;
var map = new Map();
var set = new Set();
for (var i = 0; i < N; i++) {
  map.set(i, i * 2);
  set.add(i);
}
assert.sameValue(map.size, N, "initial Map size");
assert.sameValue(set.size, N, "initial Set size");
for (var i = 0; i < N; i += 17) {
  assert.sameValue(map.get(i), i * 2, "Map.get " + i);
  assert.sameValue(set.has(i), true, "Set.has " + i);
}

// An iterator suspended mid-way survives the deletion of almost everything
// that follows and of everything it has already visited.
var mapIter = map.keys();
var setIter = set.values();
for (var i = 0; i < 10; i++) {
  assert.sameValue(mapIter.next().value, i, "Map iterator warm-up " + i);
  assert.sameValue(setIter.next().value, i, "Set iterator warm-up " + i);
}
for (var i = 0; i < N - 5; i++) {
  if (i === 12) continue;
  assert.sameValue(map.delete(i), true, "Map.delete " + i);
  assert.sameValue(set.delete(i), true, "Set.delete " + i);
}
assert.sameValue(map.delete(0), false, "Map.delete of an absent key");
assert.sameValue(set.delete(0), false, "Set.delete of an absent key");
assert.sameValue(map.size, 6, "Map size after bulk delete");
assert.sameValue(set.size, 6, "Set size after bulk delete");
map.set("late", "L");
set.add("late");
assert.compareArray(Array.from(mapIter), [12, N - 5, N - 4, N - 3, N - 2, N - 1, "late"],
  "Map iterator resumes at the next surviving entry and sees later additions");
assert.compareArray(Array.from(setIter), [12, N - 5, N - 4, N - 3, N - 2, N - 1, "late"],
  "Set iterator resumes at the next surviving entry and sees later additions");

// A deleted key that is added again goes to the end.
assert.compareArray(keysOf(map), [12, N - 5, N - 4, N - 3, N - 2, N - 1, "late"], "Map order");
map.delete(12);
map.set(12, "again");
assert.compareArray(keysOf(map), [N - 5, N - 4, N - 3, N - 2, N - 1, "late", 12], "re-added key goes last");
assert.sameValue(map.get(12), "again", "re-added key has the new value");
set.delete(12);
set.add(12);
assert.compareArray(Array.from(set), [N - 5, N - 4, N - 3, N - 2, N - 1, "late", 12], "re-added value goes last");

// Updating an existing key keeps its position.
map.set(N - 3, "updated");
assert.compareArray(keysOf(map), [N - 5, N - 4, N - 3, N - 2, N - 1, "late", 12], "set of an existing key keeps position");
assert.sameValue(map.get(N - 3), "updated", "set of an existing key replaces the value");

// forEach deleting every entry it has not reached yet, then appending.
var big = new Map();
for (var i = 0; i < 200; i++) big.set("k" + i, i);
var seen = [];
big.forEach(function (value, key) {
  seen.push(key);
  if (key === "k0") {
    for (var j = 1; j < 190; j++) big.delete("k" + j);
    big.set("tail", "t");
  }
});
assert.compareArray(seen, ["k0", "k190", "k191", "k192", "k193", "k194", "k195", "k196", "k197", "k198", "k199", "tail"],
  "forEach skips entries deleted by the callback and visits the appended one");

// Deleting the entry being visited and everything before it.
var bigSet = new Set();
for (var i = 0; i < 100; i++) bigSet.add(i);
var visited = [];
bigSet.forEach(function (value) {
  visited.push(value);
  for (var j = 0; j <= value; j++) bigSet.delete(j);
});
assert.sameValue(visited.length, 100, "every value is visited exactly once");
assert.sameValue(visited[99], 99, "values are visited in insertion order");
assert.sameValue(bigSet.size, 0, "the set ends up empty");

// A rolling window: size stays bounded and lookups stay correct.
var rolling = new Map();
var windowSet = new Set();
var W = 50;
for (var i = 0; i < 2000; i++) {
  rolling.set(i, i);
  windowSet.add("s" + i);
  if (i >= W) {
    assert.sameValue(rolling.delete(i - W), true, "window delete " + (i - W));
    assert.sameValue(windowSet.delete("s" + (i - W)), true, "window set delete " + (i - W));
  }
}
assert.sameValue(rolling.size, W, "window size");
assert.sameValue(windowSet.size, W, "window set size");
assert.sameValue(rolling.has(1999 - W), false, "evicted key");
assert.sameValue(rolling.has(1999 - W + 1), true, "oldest retained key");
assert.sameValue(windowSet.has("s1999"), true, "newest set value");
assert.sameValue(keysOf(rolling)[0], 1999 - W + 1, "oldest retained key is first");

// SameValueZero over a collection large enough to be hash-indexed.
var o1 = {};
var o2 = {};
var sym = Symbol("s");
var mixed = new Map();
for (var i = 0; i < 100; i++) mixed.set("pad" + i, i);
mixed.set(0, "zero");
mixed.set(NaN, "nan");
mixed.set("0", "string zero");
mixed.set(1n, "bigint one");
mixed.set(1, "number one");
mixed.set(o1, "o1");
mixed.set(sym, "sym");
mixed.set(true, "true");
mixed.set(undefined, "undefined");
mixed.set(null, "null");
assert.sameValue(mixed.get(-0), "zero", "-0 is +0");
assert.sameValue(mixed.get(NaN), "nan", "NaN is NaN");
assert.sameValue(mixed.get("0"), "string zero", "string key");
assert.sameValue(mixed.get(1n), "bigint one", "bigint key");
assert.sameValue(mixed.get(BigInt("1")), "bigint one", "bigint key by value");
assert.sameValue(mixed.get(1), "number one", "number key apart from bigint");
assert.sameValue(mixed.get(o1), "o1", "object key");
assert.sameValue(mixed.has(o2), false, "distinct object is another key");
assert.sameValue(mixed.get(sym), "sym", "symbol key");
assert.sameValue(mixed.get(Symbol("s")), undefined, "distinct symbol is another key");
assert.sameValue(mixed.get(true), "true", "boolean key");
assert.sameValue(mixed.has(false), false, "other boolean");
assert.sameValue(mixed.get(undefined), "undefined", "undefined key");
assert.sameValue(mixed.get(null), "null", "null key");
assert.sameValue(mixed.get("pa" + "d57"), 57, "string keys compare by content");
mixed.set(-0, "replaced");
assert.sameValue(mixed.get(0), "replaced", "-0 replaces the +0 entry");
assert.sameValue(Object.is(keysOf(mixed)[100], 0), true, "the key stays +0");
assert.sameValue(mixed.size, 110, "no duplicate entries were created");

var mixedSet = new Set(Array.from(mixed.keys()));
mixedSet.add(-0);
mixedSet.add(NaN);
mixedSet.add("pad" + 3);
assert.sameValue(mixedSet.size, 110, "Set.add of existing values changes nothing");
assert.sameValue(mixedSet.delete(-0), true, "Set.delete(-0) removes +0");
assert.sameValue(mixedSet.has(0), false, "+0 is gone");
assert.sameValue(mixedSet.delete(NaN), true, "Set.delete(NaN) removes NaN");
assert.sameValue(mixedSet.has(NaN), false, "NaN is gone");

// Set methods over large operands.
var left = new Set();
var right = new Set();
for (var i = 0; i < 400; i++) {
  left.add(i);
  if (i % 2 === 0) right.add(i);
}
assert.sameValue(left.intersection(right).size, 200, "intersection size");
assert.sameValue(left.union(right).size, 400, "union size");
assert.sameValue(left.difference(right).size, 200, "difference size");
assert.sameValue(left.symmetricDifference(right).size, 200, "symmetricDifference size");
assert.sameValue(right.isSubsetOf(left), true, "isSubsetOf");
assert.sameValue(left.isSupersetOf(right), true, "isSupersetOf");
assert.sameValue(left.difference(right).isDisjointFrom(right), true, "isDisjointFrom");
assert.compareArray(Array.from(left.difference(right)).slice(0, 3), [1, 3, 5], "difference keeps order");
assert.compareArray(Array.from(left.union(new Set([1000, 5, 999]))).slice(-2), [1000, 999], "union appends new values in order");

// Set methods whose callbacks delete and re-add entries of the receiver while
// it is large enough to be hash-indexed and compacted.
var receiver = new Set();
for (var i = 0; i < 40; i++) receiver.add(i);
var mutating = {
  size: 1000,
  has: function (v) {
    receiver.delete(v);
    for (var j = 0; j < 30; j++) receiver.delete(j);
    receiver.add(v + 1000);
    return v % 2 === 0;
  },
  keys: function () { return [][Symbol.iterator](); }
};
assert.compareArray(Array.from(receiver.difference(mutating)).filter(function (v) { return v < 1000; }),
  [1, 3, 5, 7, 9, 11, 13, 15, 17, 19, 21, 23, 25, 27, 29, 31, 33, 35, 37, 39],
  "difference works on a copy of the receiver that its callback mutates");

var computed = new Map();
for (var i = 0; i < 100; i++) computed.set(i, i);
var inserted = computed.getOrInsertComputed(500, function (key) {
  computed.set(key, "inner");
  for (var j = 0; j < 90; j++) computed.delete(j);
  return "outer";
});
assert.sameValue(inserted, "outer", "getOrInsertComputed returns the callback result");
assert.sameValue(computed.get(500), "outer", "the callback result replaces the entry it inserted");
assert.sameValue(computed.size, 11, "no duplicate entry for the callback's key");
assert.compareArray(keysOf(computed).slice(-3), [98, 99, 500], "the inserted entry keeps its position");

var groups = Map.groupBy(Array.from({ length: 300 }, function (_, n) { return n; }), function (n) { return n % 100; });
assert.sameValue(groups.size, 100, "groupBy creates one group per key");
assert.sameValue(groups.get(42).length, 3, "groupBy collects every member of a group");
assert.compareArray(groups.get(42), [42, 142, 242], "groupBy keeps members in order");
