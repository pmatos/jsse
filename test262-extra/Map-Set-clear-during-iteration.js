/*---
description: >
  Map.prototype.clear and Set.prototype.clear empty the existing entries without
  shortening the entry list, so an iterator, forEach or Set method that is
  walking the collection when it is cleared goes on to visit the entries added
  after the clear, and only those.
esid: sec-map.prototype.clear
info: |
  Map.prototype.clear (24.1.3.1) and Set.prototype.clear (24.2.4.2) set the key
  and value of every existing element to ~empty~ rather than removing them, and
  new elements are appended after them. CreateMapIterator,
  CreateSetIterator, Map.prototype.forEach, Set.prototype.forEach and the Set
  methods that walk O.[[SetData]] keep an index into that list and skip
  ~empty~ elements, so they continue with the elements added after the clear.
includes: [compareArray.js]
features: [set-methods]
---*/

var map = new Map([[1, "a"], [2, "b"], [3, "c"]]);
var mapIter = map.keys();
assert.sameValue(mapIter.next().value, 1, "first key before clear");
map.clear();
map.set(4, "d");
var step = mapIter.next();
assert.sameValue(step.done, false, "Map iterator continues after clear");
assert.sameValue(step.value, 4, "Map iterator yields the key added after clear");
assert.sameValue(mapIter.next().done, true, "Map iterator then finishes");

var set = new Set([1, 2, 3]);
var setIter = set.values();
setIter.next();
set.clear();
set.clear();
set.add(5);
set.add(6);
assert.compareArray(Array.from(setIter), [5, 6], "Set iterator visits every value added after two clears");

var entries = new Map([[1, 1], [2, 2]]);
var entriesIter = entries.entries();
entriesIter.next();
entries.delete(2);
entries.clear();
entries.set(7, 7);
entries.set(8, 8);
entries.delete(7);
var rest = Array.from(entriesIter);
assert.sameValue(rest.length, 1, "deleted and cleared entries are skipped");
assert.compareArray(rest[0], [8, 8], "the surviving entry added after clear is visited");

var finished = new Map([[1, 1]]);
var finishedIter = finished.keys();
finishedIter.next();
assert.sameValue(finishedIter.next().done, true, "iterator finished before clear");
finished.clear();
finished.set(2, 2);
assert.sameValue(finishedIter.next().done, true, "a finished iterator stays finished");

var mapSeen = [];
var forEachMap = new Map([["a", 1], ["b", 2]]);
forEachMap.forEach(function (value, key) {
  mapSeen.push(key);
  if (key === "a") {
    forEachMap.clear();
    forEachMap.set("c", 3);
  }
});
assert.compareArray(mapSeen, ["a", "c"], "Map.prototype.forEach continues with entries added after clear");

var setSeen = [];
var forEachSet = new Set(["a", "b"]);
forEachSet.forEach(function (value) {
  setSeen.push(value);
  if (value === "a") {
    forEachSet.clear();
    forEachSet.add("z");
  }
});
assert.compareArray(setSeen, ["a", "z"], "Set.prototype.forEach continues with values added after clear");

var receiver = new Set([1, 2, 3]);
var other = {
  size: 10,
  has: function (value) {
    if (value === 1) {
      receiver.clear();
      receiver.add(9);
    }
    return true;
  },
  keys: function () {
    throw new Test262Error("keys should not be called");
  }
};
assert.compareArray(Array.from(receiver.intersection(other)), [1, 9], "intersection walks values added after clear");

var subsetReceiver = new Set([1, 2]);
var subsetSeen = [];
var subsetResult = subsetReceiver.isSubsetOf({
  size: 10,
  has: function (value) {
    subsetSeen.push(value);
    if (value === 1) {
      subsetReceiver.clear();
      subsetReceiver.add(7);
    }
    return value !== 7;
  },
  keys: function () {
    throw new Test262Error("keys should not be called");
  }
});
assert.compareArray(subsetSeen, [1, 7], "isSubsetOf walks values added after clear");
assert.sameValue(subsetResult, false, "isSubsetOf sees the value added after clear");

var disjointReceiver = new Set([1, 2]);
var disjointSeen = [];
var disjointResult = disjointReceiver.isDisjointFrom({
  size: 10,
  has: function (value) {
    disjointSeen.push(value);
    if (value === 1) {
      disjointReceiver.clear();
      disjointReceiver.add(7);
    }
    return value === 7;
  },
  keys: function () {
    throw new Test262Error("keys should not be called");
  }
});
assert.compareArray(disjointSeen, [1, 7], "isDisjointFrom walks values added after clear");
assert.sameValue(disjointResult, false, "isDisjointFrom sees the value added after clear");

var refill = new Map();
for (var round = 0; round < 3; round++) {
  for (var i = 0; i < 4; i++) {
    refill.set(i, round);
  }
  var refillIter = refill.values();
  refillIter.next();
  refill.clear();
  refill.set("last", round);
  assert.compareArray(Array.from(refillIter), [round], "round " + round + " sees only the entry added after its clear");
  assert.sameValue(refill.size, 1, "round " + round + " size after clear and set");
}
