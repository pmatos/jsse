// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-enumerableownproperties
description: >
  Object.keys, Object.values and Object.entries on ordinary objects: integer
  keys ascending before string keys in creation order, Symbol keys skipped,
  non-enumerable keys skipped, and, for values/entries, [[GetOwnProperty]] is
  re-evaluated per key so a getter that deletes or hides a later key (or adds a
  new one) is observed exactly as the specification orders it.
info: |
  7.3.23 EnumerableOwnProperties ( O, kind )
    1. Let ownKeys be ? O.[[OwnPropertyKeys]]().
    3. For each element key of ownKeys, do
      a. If key is a String, then
        i. Let desc be ? O.[[GetOwnProperty]](key).
        ii. If desc is not undefined and desc.[[Enumerable]] is true, then
          1. If kind is key, append key.
          2. Else, let value be ? Get(O, key). ...
includes: [compareArray.js, propertyHelper.js]
features: [Symbol, Object.values, Object.entries]
---*/

var sym = Symbol("s");
var o = {};
o.b = 1;
o[10] = "ten";
o.a = 2;
o[2] = "two";
o[sym] = "sym";
Object.defineProperty(o, "hidden", { value: 3, enumerable: false, configurable: true });
o["01"] = "leading zero";
o[4294967295] = "not an index";
o[4294967294] = "max index";

assert.compareArray(
  Object.keys(o),
  ["2", "10", "4294967294", "b", "a", "01", "4294967295"],
  "keys: indices ascending, then strings in creation order"
);
assert.compareArray(
  Object.values(o),
  ["two", "ten", "max index", 1, 2, "leading zero", "not an index"],
  "values follow the same order"
);
var entries = Object.entries(o);
assert.sameValue(entries.length, 7);
assert.compareArray(entries[0], ["2", "two"]);
assert.compareArray(entries[3], ["b", 1]);
assert.compareArray(entries[6], ["4294967295", "not an index"]);

// A getter that deletes a later key: that key is not visited.
var log = [];
var deleter = {
  get a() { log.push("a"); delete this.c; return 1; },
  b: 2,
  c: 3,
  d: 4
};
assert.compareArray(Object.values(deleter), [1, 2, 4], "deleted later key skipped");
assert.compareArray(log, ["a"]);

// A getter that makes a later key non-enumerable.
var hider = {
  get a() { Object.defineProperty(this, "b", { enumerable: false }); return "A"; },
  b: "B",
  c: "C"
};
assert.compareArray(Object.entries(hider).map(function (e) { return e.join("="); }), ["a=A", "c=C"]);

// A getter that makes a later non-enumerable key enumerable: it is included.
var shower = {
  get a() { Object.defineProperty(this, "b", { enumerable: true }); return "A"; },
  c: "C"
};
Object.defineProperty(shower, "b", { value: "B", enumerable: false, configurable: true, writable: true });
// ownKeys snapshot is [a, c, b]; b is enumerable by the time it is visited.
assert.compareArray(Object.values(shower), ["A", "C", "B"]);

// A getter that adds a new key: not part of the snapshot.
var adder = {
  get a() { this.late = 1; return 0; },
  b: 1
};
assert.compareArray(Object.keys(adder), ["a", "b"]);
assert.compareArray(Object.values(adder), [0, 1], "added key not visited");
assert.sameValue(adder.late, 1);

// Accessors are invoked in key order, once each.
var calls = [];
var ordered = {
  get z() { calls.push("z"); return 1; },
  get y() { calls.push("y"); return 2; },
  1: "one"
};
Object.entries(ordered);
assert.compareArray(calls, ["z", "y"]);

// A throwing getter propagates.
var thrower = { a: 1, get b() { throw new Test262Error("getter"); } };
assert.throws(Test262Error, function () { Object.values(thrower); });
assert.throws(Test262Error, function () { Object.entries(thrower); });
assert.compareArray(Object.keys(thrower), ["a", "b"], "keys never invokes getters");

// Prototype properties are not own.
var child = Object.create({ inherited: 1 });
child.own = 2;
assert.compareArray(Object.keys(child), ["own"]);
assert.compareArray(Object.values(child), [2]);

// Functions, String wrappers and arguments keep their exotic keys.
function f(a, b) {}
f.extra = 1;
assert.compareArray(Object.keys(f), ["extra"]);
assert.compareArray(Object.keys(Object("ab")), ["0", "1"]);
assert.compareArray(Object.values(Object("ab")), ["a", "b"]);
(function () {
  assert.compareArray(Object.keys(arguments), ["0", "1"]);
  assert.compareArray(Object.values(arguments), ["x", "y"]);
})("x", "y");

// Arrays: elements first, then named properties.
var arr = ["p", "q"];
arr.named = "n";
assert.compareArray(Object.keys(arr), ["0", "1", "named"]);
assert.compareArray(Object.values(arr), ["p", "q", "n"]);
arr[5] = "r";
assert.compareArray(Object.keys(arr), ["0", "1", "5", "named"]);
Object.defineProperty(arr, "1", { enumerable: false });
assert.compareArray(Object.keys(arr), ["0", "5", "named"]);
assert.compareArray(Object.values(arr), ["p", "r", "n"]);

// Present undefined values are enumerated.
var withUndefined = { a: undefined, b: 1 };
assert.compareArray(Object.keys(withUndefined), ["a", "b"]);
assert.compareArray(Object.values(withUndefined), [undefined, 1]);
assert.sameValue(Object.entries([undefined]).length, 1);
