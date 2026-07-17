/*---
description: >
  Every Map.prototype method (and the `size` getter) performs
  RequireInternalSlot(M, [[MapData]]) on its this value and throws a TypeError
  when the receiver is not a Map. A plain object, a Set (which has [[SetData]],
  not [[MapData]]), a WeakMap (whose backing store is not a Map), null,
  undefined, and primitives are all rejected. Genuine Map instances — including
  instances of a Map subclass — are accepted.
info: |
  24.1.3 Properties of the Map Prototype Object

  Each Map.prototype method begins with, e.g. for Map.prototype.get:
    1. Let M be the this value.
    2. Perform ? RequireInternalSlot(M, [[MapData]]).

  RequireInternalSlot ( O, internalSlot ) throws a TypeError when O is not an
  Object or lacks the requested slot, so every method rejects a non-Map
  receiver. A Set carries [[SetData]] rather than [[MapData]]; a WeakMap's
  internal store is not a Map's [[MapData]]. Subclass instances created by
  `super()` do have [[MapData]] and are therefore accepted.

  This test also pins the engine's brand-check TypeError messages
  ("Map.prototype.<name> requires a Map"): each message embeds the method name,
  so asserting it confirms every method routes through the shared receiver guard
  with the correct method label.
esid: sec-map.prototype
features: [Symbol.iterator]
---*/

// Nullary-callable views of every brand-checked Map.prototype method, so we can
// invoke each on a hostile receiver and observe the RequireInternalSlot guard
// before any argument processing.
var mapSizeGetter = Object.getOwnPropertyDescriptor(Map.prototype, "size").get;
var methods = {
  get: function (r) { return Map.prototype.get.call(r, 1); },
  set: function (r) { return Map.prototype.set.call(r, 1, 2); },
  has: function (r) { return Map.prototype.has.call(r, 1); },
  "delete": function (r) { return Map.prototype["delete"].call(r, 1); },
  clear: function (r) { return Map.prototype.clear.call(r); },
  forEach: function (r) { return Map.prototype.forEach.call(r, function () {}); },
  entries: function (r) { return Map.prototype.entries.call(r); },
  keys: function (r) { return Map.prototype.keys.call(r); },
  values: function (r) { return Map.prototype.values.call(r); },
  size: function (r) { return mapSizeGetter.call(r); }
};
// Upsert-proposal methods are optional; test them only when present.
if (typeof Map.prototype.getOrInsert === "function") {
  methods.getOrInsert = function (r) { return Map.prototype.getOrInsert.call(r, 1, 2); };
}
if (typeof Map.prototype.getOrInsertComputed === "function") {
  methods.getOrInsertComputed =
    function (r) { return Map.prototype.getOrInsertComputed.call(r, 1, function () { return 2; }); };
}

var names = Object.keys(methods);

// 1. On a plain object, every method throws a TypeError whose message names the
//    method — the exact observable contract of the shared receiver guard.
names.forEach(function (name) {
  var threw = false;
  try {
    methods[name]({});
  } catch (e) {
    threw = true;
    if (!(e instanceof TypeError)) {
      throw new Test262Error("Map.prototype." + name + " on {}: expected TypeError, got " + e);
    }
    var expected = "Map.prototype." + name + " requires a Map";
    if (e.message !== expected) {
      throw new Test262Error(
        "Map.prototype." + name + " on {}: expected message " + JSON.stringify(expected) +
        ", got " + JSON.stringify(e.message));
    }
  }
  if (!threw) {
    throw new Test262Error("Map.prototype." + name + " on {}: expected TypeError, no throw");
  }
});

// 2. Every incompatible receiver kind is rejected with a TypeError.
var badReceivers = [
  ["Set", new Set()],
  ["WeakMap", new WeakMap()],
  ["Map.prototype", Map.prototype],
  ["null", null],
  ["undefined", undefined],
  ["number", 42],
  ["string", "map"],
  ["boolean", true],
  ["symbol", Symbol("s")]
];
names.forEach(function (name) {
  badReceivers.forEach(function (pair) {
    var threw = false;
    try {
      methods[name](pair[1]);
    } catch (e) {
      threw = e instanceof TypeError;
      if (!threw) {
        throw new Test262Error(
          "Map.prototype." + name + " on " + pair[0] + ": expected TypeError, got " + e);
      }
    }
    if (!threw) {
      throw new Test262Error(
        "Map.prototype." + name + " on " + pair[0] + ": expected TypeError, no throw");
    }
  });
});

// 3. A genuine Map receiver is accepted and behaves.
var m = new Map();
if (m.set(1, 2) !== m) throw new Test262Error("set on Map should return the map");
if (m.get(1) !== 2) throw new Test262Error("get on Map should return the stored value");
if (m.has(1) !== true) throw new Test262Error("has on Map should be true");
if (m.size !== 1) throw new Test262Error("size on Map should be 1");
if (typeof m.entries().next !== "function") throw new Test262Error("entries should return an iterator");
if (m["delete"](1) !== true) throw new Test262Error("delete on Map should return true");
if (m.size !== 0) throw new Test262Error("size after delete should be 0");

// 4. A subclass instance carries [[MapData]] and is accepted.
class MyMap extends Map {}
var sub = new MyMap();
sub.set("k", "v");
if (sub.get("k") !== "v") throw new Test262Error("get on Map subclass should work");
if (sub.has("k") !== true) throw new Test262Error("has on Map subclass should be true");
if (sub.size !== 1) throw new Test262Error("size on Map subclass should be 1");
var forEachRan = false;
sub.forEach(function (v, k) {
  forEachRan = true;
  if (v !== "v" || k !== "k") throw new Test262Error("forEach on subclass saw wrong entry");
});
if (!forEachRan) throw new Test262Error("forEach on subclass did not iterate");
