/*---
description: >
  A WeakMap value is kept alive while its key is reachable, even when the key
  only becomes reachable through another WeakMap value that is itself found
  late, so a chain of WeakMaps survives repeated full collections intact.
esid: sec-weakmap.prototype.set
info: |
  WeakMap.prototype.set (24.3.3.5) and the liveness rules of 9.10.3 (Ephemerons):
  a value is live when its key is live, transitively across WeakMaps.
features: [WeakMap, host-gc-required]
---*/

var N = 24;
var maps = [];
var keys = [];
for (var i = 0; i <= N; i++) {
  maps.push(new WeakMap());
  keys.push({ index: i });
}

// maps[i] maps keys[i] to a holder of keys[i + 1] and of maps[i + 1], so each
// WeakMap and key is reachable only through the previous entry. They are
// populated from the tail so that a collector scanning in creation order has to
// iterate to a fixpoint.
for (var i = N - 1; i >= 0; i--) {
  maps[i].set(keys[i], { nextKey: keys[i + 1], nextMap: maps[i + 1], depth: i });
}
maps[N].set(keys[N], { tail: true, payload: [1, 2, 3] });

var headMap = maps[0];
var headKey = keys[0];
maps = null;
keys = null;

function churn() {
  var junk = [];
  for (var j = 0; j < 4000; j++) {
    junk.push({ a: j, b: [j], c: "x" + j });
  }
  return junk.length;
}

function walk() {
  var map = headMap;
  var key = headKey;
  for (var depth = 0; depth < N; depth++) {
    assert.sameValue(map.has(key), true, "entry " + depth + " present");
    var holder = map.get(key);
    assert.sameValue(holder.depth, depth, "holder depth " + depth);
    assert.sameValue(holder.nextKey.index, depth + 1, "next key index " + depth);
    map = holder.nextMap;
    key = holder.nextKey;
  }
  var tail = map.get(key);
  assert.sameValue(tail.tail, true, "tail holder");
  assert.sameValue(tail.payload.join(","), "1,2,3", "tail payload");
}

walk();
for (var round = 0; round < 3; round++) {
  $262.gc();
  churn();
  walk();
}
