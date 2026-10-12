// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-object.assign
description: >
  Object.assign, object-literal spread and object rest from ordinary sources:
  String keys (integer indices first) precede Symbol keys, [[Get]] and [[Set]]
  alternate per key, an own "__proto__" data property of the source reaches
  the target's inherited __proto__ setter for assign but is created as an own
  data property by spread/rest, getters that mutate the source are observed
  per key, and write failures throw.
info: |
  20.1.2.1 Object.assign ( target, ...sources )
  7.3.26 CopyDataProperties ( target, source, excludedItems )
  13.2.5.5 PropertyDefinitionEvaluation (spread)
includes: [compareArray.js, propertyHelper.js]
features: [Symbol, object-spread, object-rest]
---*/

var s1 = Symbol("one");
var s2 = Symbol("two");
var src = {};
src[s1] = "S1";
src.b = "B";
src[7] = "seven";
src.a = "A";
src[s2] = "S2";
src[1] = "one";
Object.defineProperty(src, "hidden", { value: "H", enumerable: false, configurable: true });

var order = [];
var recorder = new Proxy({}, {
  set: function (t, k, v, r) { order.push(typeof k === "symbol" ? k.toString() : k); return Reflect.set(t, k, v, r); },
  defineProperty: function (t, k, d) { order.push("def:" + (typeof k === "symbol" ? k.toString() : k)); return Reflect.defineProperty(t, k, d); },
  getOwnPropertyDescriptor: function (t, k) { return Reflect.getOwnPropertyDescriptor(t, k); }
});
Object.assign(recorder, src);
assert.compareArray(order.filter(function (x) { return x.indexOf("def:") === 0; }),
  ["def:1", "def:7", "def:b", "def:a", "def:Symbol(one)", "def:Symbol(two)"],
  "assign writes strings (indices first) then symbols");

var copy = Object.assign({}, src);
assert.compareArray(Reflect.ownKeys(copy), ["1", "7", "b", "a", s1, s2]);
assert.sameValue(copy.hidden, undefined, "non-enumerable not copied");
verifyProperty(copy, "b", { value: "B", writable: true, enumerable: true, configurable: true }, { restore: true });

var spread = { first: 0, ...src, last: 9 };
assert.compareArray(Reflect.ownKeys(spread), ["1", "7", "first", "b", "a", "last", s1, s2]);
verifyProperty(spread, "a", { value: "A", writable: true, enumerable: true, configurable: true }, { restore: true });

var { b: omitted, ...rest } = src;
assert.compareArray(Reflect.ownKeys(rest), ["1", "7", "a", s1, s2]);
assert.sameValue(rest.b, undefined);

// Spread over existing keys keeps the original position and replaces the value.
var overwrite = { x: 1, y: 2, ...{ x: "X", z: 3 } };
assert.compareArray(Object.keys(overwrite), ["x", "y", "z"]);
assert.sameValue(overwrite.x, "X");

// Own "__proto__" source property.
var protoValue = { fromProto: true };
var withOwnProto = JSON.parse('{"__proto__": {"fromProto": true}, "k": 1}');
assert.sameValue(Object.getPrototypeOf(withOwnProto), Object.prototype);
assert.compareArray(Object.keys(withOwnProto), ["__proto__", "k"]);

var assigned = Object.assign({}, withOwnProto);
assert.sameValue(assigned.fromProto, true, "assign invoked the __proto__ setter");
assert.notSameValue(Object.getPrototypeOf(assigned), Object.prototype);
assert.compareArray(Object.keys(assigned), ["k"]);

var spreadProto = { ...withOwnProto };
assert.sameValue(Object.getPrototypeOf(spreadProto), Object.prototype, "spread does not change [[Prototype]]");
assert.sameValue(Object.prototype.hasOwnProperty.call(spreadProto, "__proto__"), true);
assert.compareArray(Object.keys(spreadProto), ["__proto__", "k"]);
assert.sameValue(spreadProto.fromProto, undefined);

var { k: dropK, ...restProto } = withOwnProto;
assert.sameValue(Object.getPrototypeOf(restProto), Object.prototype);
assert.sameValue(Object.prototype.hasOwnProperty.call(restProto, "__proto__"), true);

// Getters mutating the source mid-copy.
function makeMutating(log) {
  return {
    get a() { log.push("get a"); delete this.c; this.late = 1; return "A"; },
    b: "B",
    c: "C",
    d: "D"
  };
}
var log = [];
var mutating = makeMutating(log);
assert.compareArray(Object.keys(Object.assign({}, mutating)), ["a", "b", "d"]);
assert.compareArray(log, ["get a"]);
assert.sameValue(mutating.late, 1);
log = [];
mutating = makeMutating(log);
assert.compareArray(Object.keys({ ...mutating }), ["a", "b", "d"]);
assert.compareArray(log, ["get a"]);
assert.sameValue(mutating.late, 1);

// A setter on the target runs between source reads.
var events = [];
var tgt = {
  set a(v) { events.push("set a=" + v); delete source2.b; },
  set b(v) { events.push("set b=" + v); }
};
var source2 = {
  get a() { events.push("get a"); return 1; },
  get b() { events.push("get b"); return 2; },
  get c() { events.push("get c"); return 3; }
};
Object.assign(tgt, source2);
assert.compareArray(events, ["get a", "set a=1", "get c"],
  "b deleted by setter before it is read");
assert.sameValue(tgt.c, 3);

// Inherited non-writable property on the target: assignment throws.
var guarded = Object.create(Object.defineProperty({}, "x", { value: 1, writable: false }));
assert.throws(TypeError, function () { Object.assign(guarded, { x: 2 }); });
assert.sameValue(Object.assign(guarded, { y: 2 }), guarded);
assert.sameValue(guarded.y, 2);

// Frozen target.
assert.throws(TypeError, function () { Object.assign(Object.freeze({}), { a: 1 }); });
Object.assign(Object.freeze({ a: 1 }), {});

// Multiple sources, nullish skipped, primitives wrapped.
var multi = Object.assign({}, { a: 1 }, null, undefined, "xy", 5, { b: 2 });
assert.compareArray(Object.keys(multi), ["0", "1", "a", "b"]);
assert.sameValue(multi[0], "x");

// Target equal to source.
var same = { p: 1, q: 2 };
assert.sameValue(Object.assign(same, same), same);
assert.compareArray(Object.keys(same), ["p", "q"]);

// Many keys (past the inline property storage) round-trip in order.
var big = {};
for (var i = 0; i < 40; i++) big["k" + i] = i;
var bigCopy = Object.assign({}, big);
var bigSpread = { ...big, extra: 1 };
assert.compareArray(Object.keys(bigCopy), Object.keys(big));
assert.compareArray(Object.keys(bigSpread), Object.keys(big).concat(["extra"]));
assert.sameValue(bigSpread.k39, 39);
assert.sameValue(bigCopy.k0, 0);
delete bigCopy.k5;
bigCopy.k5 = "again";
assert.sameValue(Object.keys(bigCopy)[39], "k5");
