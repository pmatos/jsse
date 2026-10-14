/*---
description: >
  Objects stored into a large, long-lived Map or Set stay reachable and keep
  their identity across minor collections, however the entries were written:
  overwriting a key, appending, getOrInsert, Set add, deleting enough entries to
  compact the entry list, and clearing and refilling.
info: |
  The generational collector remembers an old Map's or Set's entry writes per
  card (a fixed-size run of entry slots) rather than re-scanning the whole
  entry list. A store that skipped its card, or a compaction that moved a
  young entry into a card nobody had marked, would let the collector free an
  object the collection still references: later reads would see a wrong value
  or throw. Only the observable result is asserted here: every entry must
  still hold the object last written to it.
esid: sec-map-objects
features: [upsert, host-gc-required]
---*/

var seed = 987654;
function next(n) {
  seed = (Math.imul(seed, 1103515245) + 12345) & 0x7fffffff;
  return seed % n;
}

function churn() {
  // Short-lived garbage so the nursery fills and minor collections run.
  var junk;
  for (var k = 0; k < 64; k++) junk = { k: k, next: junk };
  return junk;
}

var N = 2500;
var map = new Map();
var expectedMap = new Array(N);
var set = new Set();
var expectedSet = new Array(N);
for (var i = 0; i < N; i++) {
  map.set(i, { id: i });
  expectedMap[i] = i;
  set.add({ id: i });
  expectedSet[i] = i;
}
var setMembers = Array.from(set);

// Promote both collections and their records, so later stores are
// old-to-young.
$262.gc();
$262.gc();

function checkMap(label) {
  var seen = 0;
  map.forEach(function (record, key) {
    assert.sameValue(record.id, expectedMap[key], label + ": key " + key);
    seen++;
  });
  var live = 0;
  for (var i = 0; i < expectedMap.length; i++) {
    if (expectedMap[i] !== undefined) live++;
  }
  assert.sameValue(seen, live, label + ": size");
  assert.sameValue(map.size, live, label + ": map.size");
}

function checkSet(label) {
  var seen = 0;
  set.forEach(function (record) {
    assert.sameValue(typeof record.id, "number", label + ": record");
    seen++;
  });
  assert.sameValue(seen, set.size, label + ": size");
  setMembers.forEach(function (record) {
    assert.sameValue(set.has(record), true, label + ": member present");
  });
}

var serial = N;
for (var r = 0; r < 4000; r++) {
  var key = next(N);
  map.set(key, { id: serial });
  expectedMap[key] = serial++;
  if (r % 16 === 0) churn();
}
checkMap("after random overwrites");

for (var a = 0; a < 600; a++) {
  var fresh = N + a;
  map.set(fresh, { id: serial });
  expectedMap[fresh] = serial++;
  if (a % 8 === 0) churn();
}
checkMap("after appends");

for (var g = 0; g < 300; g++) {
  var gk = N + 1000 + g;
  var got = map.getOrInsert(gk, { id: serial });
  expectedMap[gk] = serial++;
  assert.sameValue(got.id, expectedMap[gk], "getOrInsert result");
  if (g % 8 === 0) churn();
}
checkMap("after getOrInsert");

for (var h = 0; h < 300; h++) {
  var hk = N + 2000 + h;
  var made = map.getOrInsertComputed(hk, function () {
    return { id: serial };
  });
  expectedMap[hk] = serial++;
  assert.sameValue(made.id, expectedMap[hk], "getOrInsertComputed result");
  if (h % 8 === 0) churn();
}
checkMap("after getOrInsertComputed");

for (var s = 0; s < 800; s++) {
  var member = { id: s };
  set.add(member);
  setMembers.push(member);
  if (s % 8 === 0) churn();
}
checkSet("after Set adds");

// Deleting most entries compacts the entry list, which moves the surviving
// (young) entries into cards that no store marked.
for (var d = 0; d < N; d++) {
  if (d % 50 !== 0) {
    map.delete(d);
    expectedMap[d] = undefined;
  }
  if (d % 32 === 0) churn();
}
map.set(7, { id: serial });
expectedMap[7] = serial++;
checkMap("after deleting");

var kept = [];
setMembers.forEach(function (record, index) {
  if (index % 40 === 0) {
    kept.push(record);
  } else {
    set.delete(record);
  }
});
setMembers = kept;
for (var c = 0; c < 20; c++) churn();
checkSet("after Set deletes");

// A cleared collection refilled with young objects is still scanned.
map.clear();
expectedMap = [];
set.clear();
setMembers = [];
for (var f = 0; f < 700; f++) {
  map.set(f, { id: serial });
  expectedMap[f] = serial++;
  var m = { id: serial++ };
  set.add(m);
  setMembers.push(m);
  if (f % 8 === 0) churn();
}
checkMap("after clear and refill");
checkSet("after clear and refill");

for (var e = 0; e < 4; e++) {
  $262.gc();
  churn();
  checkMap("after collection " + e);
  checkSet("after collection " + e);
}
