// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-enumerate-object-properties
description: >
  for-in key snapshots on ordinary objects: own integer keys ascending then
  string keys in creation order, inherited enumerable keys after own ones, a
  non-enumerable own key shadows an enumerable inherited key of the same name,
  keys deleted during iteration are skipped, keys added are not visited, and
  Object.prototype.hasOwnProperty / propertyIsEnumerable / Object.hasOwn agree
  with the enumeration.
info: |
  14.7.5.9 EnumerateObjectProperties ( O )
  The enumerable property names of prototype objects must be obtained by
  invoking EnumerateObjectProperties on the prototype object. A property name
  will be returned at most once, and a name shadowed by an own property of a
  closer object (enumerable or not) is not returned for the farther object.
includes: [compareArray.js]
---*/

function keysOf(o) {
  var out = [];
  for (var k in o) out.push(k);
  return out;
}

// Own ordering.
var o = { b: 1, 2: "x", a: 2, 1: "y" };
o.c = 3;
o[0] = "z";
assert.compareArray(keysOf(o), ["0", "1", "2", "b", "a", "c"]);

// Inherited enumerable keys follow own keys, nearest prototype first.
var grand = { g: 1, shared: "grand" };
var parent = Object.create(grand);
parent.p = 1;
parent.shared = "parent";
var child = Object.create(parent);
child.c = 1;
assert.compareArray(keysOf(child), ["c", "p", "shared", "g"]);

// Non-enumerable own key shadows an enumerable inherited one.
var base = { visible: 1, hidden: 2, other: 3 };
var derived = Object.create(base);
Object.defineProperty(derived, "hidden", { value: "own", enumerable: false });
derived.own = 1;
assert.compareArray(keysOf(derived), ["own", "visible", "other"]);

// A non-enumerable key on an intermediate prototype shadows the farther one.
var far = { x: 1, y: 2 };
var mid = Object.create(far);
Object.defineProperty(mid, "x", { value: 0, enumerable: false });
var near = Object.create(mid);
assert.compareArray(keysOf(near), ["y"]);

// Enumerable properties on Object.prototype are found (and a plain object with
// none keeps its own keys unchanged).
Object.prototype.__forInProbe = 1;
try {
  assert.compareArray(keysOf({ a: 1 }), ["a", "__forInProbe"]);
  assert.compareArray(keysOf([7]), ["0", "__forInProbe"]);
  assert.compareArray(keysOf(Object.create(null)), []);
} finally {
  delete Object.prototype.__forInProbe;
}
assert.compareArray(keysOf({ a: 1 }), ["a"]);

// Mutation during iteration.
var mutable = { a: 1, b: 2, c: 3, d: 4 };
var seen = [];
for (var k in mutable) {
  seen.push(k);
  if (k === "a") {
    delete mutable.c;
    mutable.e = 5;
  }
}
assert.compareArray(seen, ["a", "b", "d"]);

// Array-likes and exotics.
assert.compareArray(keysOf("ab"), ["0", "1"]);
assert.compareArray(keysOf(Object("ab")), ["0", "1"]);
var arr = [1, 2];
arr.extra = 1;
arr[4] = 5;
assert.compareArray(keysOf(arr), ["0", "1", "4", "extra"]);
(function () {
  assert.compareArray(keysOf(arguments), ["0", "1"]);
})("p", "q");

// Primitive keys are Strings.
for (var key in { 5: 1 }) {
  assert.sameValue(typeof key, "string");
  assert.sameValue(key, "5");
}

// Own-property predicates agree with the key snapshot.
var probe = Object.create({ inherited: 1 });
probe.own = 1;
Object.defineProperty(probe, "quiet", { value: 1, enumerable: false });
assert.sameValue(probe.hasOwnProperty("own"), true);
assert.sameValue(probe.hasOwnProperty("quiet"), true);
assert.sameValue(probe.hasOwnProperty("inherited"), false);
assert.sameValue(probe.hasOwnProperty("absent"), false);
assert.sameValue(Object.hasOwn(probe, "quiet"), true);
assert.sameValue(Object.hasOwn(probe, "inherited"), false);
assert.sameValue(probe.propertyIsEnumerable("own"), true);
assert.sameValue(probe.propertyIsEnumerable("quiet"), false);
assert.sameValue(probe.propertyIsEnumerable("inherited"), false);

var s = Object("abc");
assert.sameValue(s.hasOwnProperty("length"), true);
assert.sameValue(s.hasOwnProperty("1"), true);
assert.sameValue(s.hasOwnProperty("3"), false);
assert.sameValue(s.propertyIsEnumerable("1"), true);
assert.sameValue(s.propertyIsEnumerable("length"), false);

var a2 = [undefined, , 3];
assert.sameValue(a2.hasOwnProperty("0"), true, "present undefined element");
assert.sameValue(a2.hasOwnProperty("1"), false, "hole");
assert.sameValue(a2.hasOwnProperty("2"), true);
assert.sameValue(a2.hasOwnProperty("length"), true);
assert.sameValue(a2.propertyIsEnumerable("length"), false);
assert.sameValue(a2.propertyIsEnumerable(2), true);

var sym = Symbol("probe");
var withSym = {};
withSym[sym] = 1;
assert.sameValue(withSym.hasOwnProperty(sym), true);
assert.sameValue(withSym.propertyIsEnumerable(sym), true);
assert.compareArray(keysOf(withSym), []);
