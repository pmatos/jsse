// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-arguments-exotic-objects
description: >
  Computed numeric-index reads of arguments objects and ordinary objects, and
  the parameter map of mapped arguments objects: live reads through the map,
  writes through to parameters, unmapped and non-canonical index keys, a
  parameter list longer than any inline mapping flags, duplicate parameter
  names, an own accessor index, an inherited index, and named properties of a
  mapped object.
info: |
  10.4.4.1 [[GetOwnProperty]] ( P )
  10.4.4.2 [[DefineOwnProperty]] ( P, Desc )
  10.4.4.3 [[Get]] ( P, Receiver )
  10.4.4.4 [[Set]] ( P, V, Receiver )
  10.4.4.5 [[Delete]] ( P )
flags: [noStrict]
features: [Reflect]
---*/

function readsLive(a, b) {
  var before = arguments[0] + ":" + arguments[1];
  a = "A";
  b = "B";
  return before + "|" + arguments[0] + ":" + arguments[1];
}
assert.sameValue(readsLive(1, 2), "1:2|A:B", "index read follows parameter writes");

function writesThrough(a, b) {
  arguments[0] = "x";
  arguments[1] = "y";
  return a + b;
}
assert.sameValue(writesThrough(1, 2), "xy", "index write reaches the parameters");

function fractionalAndNegative(a) {
  return [arguments[0.5], arguments[-1], arguments[-0], arguments[1], arguments[4294967295]];
}
var odd = fractionalAndNegative("v");
assert.sameValue(odd[0], undefined, "fractional index");
assert.sameValue(odd[1], undefined, "negative index");
assert.sameValue(odd[2], "v", "-0 reads index 0");
assert.sameValue(odd[3], undefined, "past the end");
assert.sameValue(odd[4], undefined, "2^32 - 1 is not an array index");

function nonCanonicalKeys(a) {
  a = "param";
  return [arguments["00"], arguments["+0"], arguments["0 "], arguments["0"]];
}
var keys = nonCanonicalKeys("orig");
assert.sameValue(keys[0], undefined, '"00" is not index 0');
assert.sameValue(keys[1], undefined, '"+0" is not index 0');
assert.sameValue(keys[2], undefined, '"0 " is not index 0');
assert.sameValue(keys[3], "param", '"0" is index 0');

function nonCanonicalWriteDoesNotAlias(a) {
  arguments["00"] = "other";
  arguments["+0"] = "plus";
  return a + ":" + arguments["00"] + ":" + arguments["+0"];
}
assert.sameValue(nonCanonicalWriteDoesNotAlias("kept"), "kept:other:plus", "non-canonical keys are plain properties");

function manyParameters(p0, p1, p2, p3, p4, p5, p6, p7, p8, p9, p10, p11) {
  p9 = "nine";
  p11 = "eleven";
  arguments[10] = "ten";
  return [arguments[9], arguments[10], arguments[11], p10, arguments.length];
}
var many = manyParameters(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11);
assert.sameValue(many[0], "nine", "index 9 mapped");
assert.sameValue(many[1], "ten", "index 10 mapped");
assert.sameValue(many[2], "eleven", "index 11 mapped");
assert.sameValue(many[3], "ten", "parameter 10 aliased");
assert.sameValue(many[4], 12, "length");

function manyParametersShortCall(p0, p1, p2, p3, p4, p5, p6, p7, p8, p9, p10) {
  p10 = "late";
  arguments[10] = "own";
  return [arguments.length, arguments[10], p10];
}
var short = manyParametersShortCall(0, 1);
assert.sameValue(short[0], 2, "length");
assert.sameValue(short[1], "own", "unmapped index holds its own value");
assert.sameValue(short[2], "late", "parameter is not aliased past the argument count");

function duplicates(a, b, a, b) {
  a = "a";
  b = "b";
  return [arguments[0], arguments[1], arguments[2], arguments[3]];
}
var dup = duplicates(1, 2, 3, 4);
assert.sameValue(dup[0], 1, "earlier duplicate is unmapped");
assert.sameValue(dup[1], 2, "earlier duplicate is unmapped");
assert.sameValue(dup[2], "a", "last duplicate is mapped");
assert.sameValue(dup[3], "b", "last duplicate is mapped");

function severedByDelete(a, b) {
  delete arguments[0];
  a = "after";
  b = "later";
  return [arguments[0], arguments[1], 0 in arguments];
}
var severed = severedByDelete(1, 2);
assert.sameValue(severed[0], undefined, "deleted index reads undefined");
assert.sameValue(severed[1], "later", "other index stays mapped");
assert.sameValue(severed[2], false, "deleted index is absent");

function severedByReflect(a) {
  Reflect.deleteProperty(arguments, "0");
  arguments[0] = "own";
  a = "param";
  return arguments[0] + ":" + a;
}
assert.sameValue(severedByReflect(1), "own:param", "Reflect.deleteProperty severs the mapping");

function ownAccessor(a) {
  Object.defineProperty(arguments, "0", {
    get: function () { return "getter"; },
    configurable: true,
  });
  return arguments[0];
}
assert.sameValue(ownAccessor("v"), "getter", "an own accessor index runs its getter");

function frozenIndex(a) {
  Object.defineProperty(arguments, "0", { value: "frozen", writable: false });
  a = "param";
  return arguments[0];
}
assert.sameValue(frozenIndex("v"), "frozen", "a non-writable index keeps its value");

Object.prototype[5] = "inherited";
try {
  function inheritedIndex() {
    return [arguments[0], arguments[5], 5 in arguments, arguments.hasOwnProperty(5)];
  }
  var inh = inheritedIndex("own");
  assert.sameValue(inh[0], "own", "own index");
  assert.sameValue(inh[1], "inherited", "absent index reads the prototype chain");
  assert.sameValue(inh[2], true, "in sees the inherited index");
  assert.sameValue(inh[3], false, "not an own property");

  var ordinary = { 0: "zero" };
  assert.sameValue(ordinary[0], "zero", "ordinary own index");
  assert.sameValue(ordinary[5], "inherited", "ordinary inherited index");
} finally {
  delete Object.prototype[5];
}

var accessorHolder = {
  get 3() { return "from getter " + (this === accessorHolder); },
};
assert.sameValue(accessorHolder[3], "from getter true", "ordinary accessor index receives the receiver");

var inheritedAccessor = Object.create({
  get 7() { return this.tag; },
});
inheritedAccessor.tag = "tagged";
assert.sameValue(inheritedAccessor[7], "tagged", "inherited accessor index receives the receiver");

var holey = { 0: "a", 2: "c" };
assert.sameValue(holey[1], undefined, "missing ordinary index");
assert.sameValue(holey[2], "c", "ordinary index after a gap");

var withUndefined = { 4: undefined };
assert.sameValue(withUndefined[4], undefined, "own undefined index");
assert.sameValue(4 in withUndefined, true, "own undefined index is present");

function namedOnMapped(a) {
  arguments.extra = "named";
  arguments.length = 7;
  return [arguments.extra, arguments.length, arguments.callee === namedOnMapped, "extra" in arguments];
}
var named = namedOnMapped("v");
assert.sameValue(named[0], "named", "named data property");
assert.sameValue(named[1], 7, "length is a plain writable property");
assert.sameValue(named[2], true, "callee");
assert.sameValue(named[3], true, "in");

Object.defineProperty(Object.prototype, "inheritedName", {
  get: function () { return "proto getter"; },
  configurable: true,
});
try {
  function inheritedNamed(a) {
    return arguments.inheritedName;
  }
  assert.sameValue(inheritedNamed(1), "proto getter", "named miss walks to the prototype of a mapped object");
} finally {
  delete Object.prototype.inheritedName;
}

function namedStoreOnMapped(a) {
  arguments.fresh = 1;
  arguments.fresh += 2;
  return arguments.fresh;
}
assert.sameValue(namedStoreOnMapped("v"), 3, "named add and update on a mapped object");

function replaceThroughApply(a, b) {
  a = "changed";
  return Array.prototype.slice.call(arguments).join();
}
assert.sameValue(replaceThroughApply(1, 2), "changed,2", "slice reads live mapped values");

function escapedMap(a) {
  return arguments;
}
var escaped = escapedMap("kept");
assert.sameValue(escaped[0], "kept", "escaped arguments read after the call returns");
escaped[0] = "written";
assert.sameValue(escaped[0], "written", "escaped arguments write after the call returns");

function collect() {
  return Array.prototype.slice.call(arguments).join();
}

function applyMapped(a, b) {
  a = 5;
  return collect.apply(this, arguments);
}
assert.sameValue(applyMapped(1, 2, 3), "5,2,3", "apply reads the live mapped parameters");

function applyUnmapped(a) {
  "use strict";
  a = 5;
  return collect.apply(this, arguments);
}
assert.sameValue(applyUnmapped(1, 2), "1,2", "apply of an unmapped arguments object");

function applyShort(a, b, c) {
  b = "late";
  return collect.apply(null, arguments);
}
assert.sameValue(applyShort(1), "1", "unmapped parameter past the argument count is not passed");

function countArguments() {
  return arguments.length;
}
assert.sameValue(countArguments.apply(null, [1, , 3]), 3, "sparse array length");
Array.prototype[1] = "inherited";
try {
  assert.sameValue(collect.apply(null, [1, , 3]), "1,inherited,3", "apply of a hole reads the prototype chain");
} finally {
  delete Array.prototype[1];
}
assert.sameValue(collect.apply(null, [1, , 3]), "1,,3", "apply of a hole without an inherited element");

var log = [];
var arrayLike = {
  length: 3,
  get 0() { log.push("0"); return "a"; },
  get 1() { log.push("1"); return "b"; },
  2: "c",
};
assert.sameValue(collect.apply(null, arrayLike), "a,b,c", "apply of an array-like with accessor indices");
assert.sameValue(log.join(), "0,1", "accessors run in index order");

var proxied = new Proxy({ length: 2, 0: "x", 1: "y" }, {
  get: function (target, key, receiver) {
    log.push(String(key));
    return Reflect.get(target, key, receiver);
  },
});
log = [];
assert.sameValue(collect.apply(null, proxied), "x,y", "apply of a proxy array-like");
assert.sameValue(log.join(), "length,0,1", "proxy get trap sees length then each index");

var sealedArguments = (function () {
  Object.freeze(arguments);
  return collect.apply(null, arguments);
})("p", "q");
assert.sameValue(sealedArguments, "p,q", "apply of a frozen arguments object");
