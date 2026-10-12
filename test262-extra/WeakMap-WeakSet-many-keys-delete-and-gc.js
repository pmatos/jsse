/*---
description: >
  WeakMap and WeakSet answer get, has, set and delete correctly for many
  live keys, after deleting most of them, and after a collection reclaims the
  unreachable ones, without ever mistaking one key for another.
esid: sec-weakmap.prototype.get
info: |
  WeakMap.prototype.get (24.3.3.3), WeakMap.prototype.set (24.3.3.5) and
  WeakSet.prototype.add (24.4.3.1) compare keys by SameValue, so distinct
  objects and symbols are distinct keys however many are stored. Entries whose
  key is unreachable may be dropped without affecting the others.
features: [WeakMap, WeakSet, Symbol, symbols-as-weakmap-keys, upsert, host-gc-required]
---*/

var N = 500;
var keys = [];
var wm = new WeakMap();
var ws = new WeakSet();
for (var i = 0; i < N; i++) {
  var k = { id: i };
  keys.push(k);
  wm.set(k, i);
  ws.add(k);
}
for (var i = 0; i < N; i++) {
  assert.sameValue(wm.get(keys[i]), i, "get " + i);
  assert.sameValue(wm.has(keys[i]), true, "has " + i);
  assert.sameValue(ws.has(keys[i]), true, "WeakSet.has " + i);
}
assert.sameValue(wm.has({ id: 0 }), false, "an equal-looking object is another key");
assert.sameValue(ws.has({ id: 0 }), false, "WeakSet: an equal-looking object is another key");

wm.set(keys[7], "seven");
assert.sameValue(wm.get(keys[7]), "seven", "set replaces the value of an existing key");
ws.add(keys[7]);
assert.sameValue(wm.getOrInsert(keys[8], "unused"), 8, "getOrInsert returns the existing value");

for (var i = 0; i < N; i += 2) {
  assert.sameValue(wm.delete(keys[i]), true, "delete " + i);
  assert.sameValue(ws.delete(keys[i]), true, "WeakSet.delete " + i);
}
for (var i = 0; i < N; i++) {
  var expected = i % 2 === 1;
  assert.sameValue(wm.has(keys[i]), expected, "has after delete " + i);
  assert.sameValue(ws.has(keys[i]), expected, "WeakSet.has after delete " + i);
}
assert.sameValue(wm.delete(keys[0]), false, "delete of an absent key");

wm.set(keys[0], "back");
assert.sameValue(wm.get(keys[0]), "back", "a deleted key can be set again");

// Drop references to most keys and collect; the survivors must still answer.
var survivors = [];
for (var i = 1; i < N; i += 10) survivors.push(keys[i]);
keys = null;
$262.gc();
for (var i = 0; i < survivors.length; i++) {
  var idx = 1 + i * 10;
  assert.sameValue(wm.get(survivors[i]), idx, "survivor " + idx + " keeps its value");
  assert.sameValue(ws.has(survivors[i]), true, "survivor " + idx + " stays in the WeakSet");
}

// Symbols as keys share the same table as objects.
var symbols = [];
var symMap = new WeakMap();
for (var i = 0; i < 100; i++) {
  var s = Symbol("s" + i);
  symbols.push(s);
  symMap.set(s, i);
}
for (var i = 0; i < 100; i++) {
  assert.sameValue(symMap.get(symbols[i]), i, "symbol key " + i);
}
assert.sameValue(symMap.has(Symbol("s0")), false, "a distinct symbol is another key");

// New keys keep working after a collection removed others.
for (var i = 0; i < 100; i++) {
  var fresh = { i: i };
  wm.set(fresh, "fresh" + i);
  assert.sameValue(wm.get(fresh), "fresh" + i, "fresh key " + i);
}
