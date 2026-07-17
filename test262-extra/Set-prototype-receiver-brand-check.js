/*---
description: >
  Every Set.prototype method (and the `size` getter) performs
  RequireInternalSlot(S, [[SetData]]) on its this value and throws a TypeError
  when the receiver is not a Set. A plain object, a Map (which has [[MapData]],
  not [[SetData]]), a WeakSet (whose backing store is not a Set's), null,
  undefined, and primitives are all rejected. Genuine Set instances — including
  instances of a Set subclass — are accepted.
info: |
  24.2.3 Properties of the Set Prototype Object

  Each Set.prototype method begins with, e.g. for Set.prototype.add:
    1. Let S be the this value.
    2. Perform ? RequireInternalSlot(S, [[SetData]]).

  RequireInternalSlot ( O, internalSlot ) throws a TypeError when O is not an
  Object or lacks the requested slot, so every method rejects a non-Set
  receiver. A Map carries [[MapData]] rather than [[SetData]]; a WeakSet's
  internal store is not a Set's [[SetData]]. The set-operation methods
  (union, intersection, ...) perform the same brand check before touching their
  argument. Subclass instances created by `super()` do have [[SetData]] and are
  therefore accepted.

  This test also pins the engine's brand-check TypeError messages
  ("Set.prototype.<name> requires a Set"): each message embeds the method name,
  so asserting it confirms every method routes through the shared receiver guard
  with the correct method label.
esid: sec-set.prototype
features: [set-methods, Symbol.iterator]
---*/

var setSizeGetter = Object.getOwnPropertyDescriptor(Set.prototype, "size").get;
var methods = {
  add: function (r) { return Set.prototype.add.call(r, 1); },
  has: function (r) { return Set.prototype.has.call(r, 1); },
  "delete": function (r) { return Set.prototype["delete"].call(r, 1); },
  clear: function (r) { return Set.prototype.clear.call(r); },
  forEach: function (r) { return Set.prototype.forEach.call(r, function () {}); },
  entries: function (r) { return Set.prototype.entries.call(r); },
  keys: function (r) { return Set.prototype.keys.call(r); },
  values: function (r) { return Set.prototype.values.call(r); },
  size: function (r) { return setSizeGetter.call(r); }
};
// ES2025 set-composition methods take a set-like argument; the brand check on
// the receiver runs first, so a valid argument still surfaces the guard.
var setOps = ["union", "intersection", "difference", "symmetricDifference",
  "isSubsetOf", "isSupersetOf", "isDisjointFrom"];
setOps.forEach(function (op) {
  if (typeof Set.prototype[op] === "function") {
    methods[op] = function (r) { return Set.prototype[op].call(r, new Set([1])); };
  }
});

var names = Object.keys(methods);

// Set.prototype.keys is the very same function object as Set.prototype.values
// (24.2.3), so its brand-check message names "values". Map has no such alias.
var canonicalName = { keys: "values" };

// 1. On a plain object, every method throws a TypeError whose message names the
//    method (or its alias target) — the exact observable contract of the shared
//    receiver guard.
names.forEach(function (name) {
  var threw = false;
  try {
    methods[name]({});
  } catch (e) {
    threw = true;
    if (!(e instanceof TypeError)) {
      throw new Test262Error("Set.prototype." + name + " on {}: expected TypeError, got " + e);
    }
    var expected = "Set.prototype." + (canonicalName[name] || name) + " requires a Set";
    if (e.message !== expected) {
      throw new Test262Error(
        "Set.prototype." + name + " on {}: expected message " + JSON.stringify(expected) +
        ", got " + JSON.stringify(e.message));
    }
  }
  if (!threw) {
    throw new Test262Error("Set.prototype." + name + " on {}: expected TypeError, no throw");
  }
});

// 2. Every incompatible receiver kind is rejected with a TypeError.
var badReceivers = [
  ["Map", new Map()],
  ["WeakSet", new WeakSet()],
  ["Set.prototype", Set.prototype],
  ["null", null],
  ["undefined", undefined],
  ["number", 42],
  ["string", "set"],
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
          "Set.prototype." + name + " on " + pair[0] + ": expected TypeError, got " + e);
      }
    }
    if (!threw) {
      throw new Test262Error(
        "Set.prototype." + name + " on " + pair[0] + ": expected TypeError, no throw");
    }
  });
});

// 3. A genuine Set receiver is accepted and behaves.
var s = new Set();
if (s.add(1) !== s) throw new Test262Error("add on Set should return the set");
if (s.has(1) !== true) throw new Test262Error("has on Set should be true");
if (s.size !== 1) throw new Test262Error("size on Set should be 1");
if (typeof s.values().next !== "function") throw new Test262Error("values should return an iterator");
if (s["delete"](1) !== true) throw new Test262Error("delete on Set should return true");
if (s.size !== 0) throw new Test262Error("size after delete should be 0");
if (typeof Set.prototype.union === "function") {
  var u = new Set([1, 2]).union(new Set([2, 3]));
  if (u.size !== 3 || !u.has(1) || !u.has(2) || !u.has(3)) {
    throw new Test262Error("union on Set should produce {1,2,3}");
  }
}

// 4. A subclass instance carries [[SetData]] and is accepted.
class MySet extends Set {}
var sub = new MySet();
sub.add("x");
if (sub.has("x") !== true) throw new Test262Error("has on Set subclass should be true");
if (sub.size !== 1) throw new Test262Error("size on Set subclass should be 1");
var forEachRan = false;
sub.forEach(function (v) {
  forEachRan = true;
  if (v !== "x") throw new Test262Error("forEach on subclass saw wrong value");
});
if (!forEachRan) throw new Test262Error("forEach on subclass did not iterate");
