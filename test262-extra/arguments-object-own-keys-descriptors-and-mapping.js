// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-createmappedargumentsobject
description: >
  Own keys, property descriptors and parameter mapping of mapped and unmapped
  arguments objects: index keys then length then callee then @@iterator,
  attribute shapes, aliasing in both directions, aliasing severed by delete or
  attribute changes, unmapped extra/missing arguments, duplicate parameter
  names, and the strict poison-pill callee.
info: |
  10.4.4.6 CreateUnmappedArgumentsObject ( argumentsList )
  10.4.4.7 CreateMappedArgumentsObject ( func, formals, argumentsList, env )
  10.4.4.7.1 MakeArgGetter / 10.4.4.7.2 MakeArgSetter
includes: [propertyHelper.js, compareArray.js]
flags: [noStrict]
features: [Symbol.iterator, Reflect, arrow-function]
---*/

function mappedKeys() {
  return Reflect.ownKeys(arguments);
}
var keys = mappedKeys("a", "b", "c");
assert.sameValue(keys.length, 6, "mapped own key count");
assert.sameValue(keys[0], "0");
assert.sameValue(keys[1], "1");
assert.sameValue(keys[2], "2");
assert.sameValue(keys[3], "length");
assert.sameValue(keys[4], "callee");
assert.sameValue(keys[5], Symbol.iterator);

function mappedDescriptors(a, b) {
  var d0 = Object.getOwnPropertyDescriptor(arguments, "0");
  assert.sameValue(d0.value, "x");
  assert.sameValue(d0.writable, true);
  assert.sameValue(d0.enumerable, true);
  assert.sameValue(d0.configurable, true);

  var dl = Object.getOwnPropertyDescriptor(arguments, "length");
  assert.sameValue(dl.value, 3);
  assert.sameValue(dl.writable, true);
  assert.sameValue(dl.enumerable, false);
  assert.sameValue(dl.configurable, true);

  var dc = Object.getOwnPropertyDescriptor(arguments, "callee");
  assert.sameValue(dc.value, mappedDescriptors);
  assert.sameValue(dc.writable, true);
  assert.sameValue(dc.enumerable, false);
  assert.sameValue(dc.configurable, true);

  var di = Object.getOwnPropertyDescriptor(arguments, Symbol.iterator);
  assert.sameValue(di.value, Array.prototype.values);
  assert.sameValue(di.writable, true);
  assert.sameValue(di.enumerable, false);
  assert.sameValue(di.configurable, true);

  assert.compareArray(Object.keys(arguments), ["0", "1", "2"], "Object.keys");
  assert.sameValue(JSON.stringify(arguments), '{"0":"x","1":"y","2":"z"}');
  assert.compareArray(Array.prototype.slice.call(arguments), ["x", "y", "z"]);
  assert.compareArray([...arguments], ["x", "y", "z"], "spread");
  assert.sameValue(Object.prototype.toString.call(arguments), "[object Arguments]");
}
mappedDescriptors("x", "y", "z");

function aliasForward(a, b) {
  a = "changed";
  return arguments[0] + ":" + arguments[1];
}
assert.sameValue(aliasForward(1, 2), "changed:2", "parameter write visible through arguments");

function aliasBackward(a, b) {
  arguments[1] = "set";
  return a + ":" + b;
}
assert.sameValue(aliasBackward(1, 2), "1:set", "arguments write visible through parameter");

function aliasDeleted(a) {
  delete arguments[0];
  arguments[0] = "again";
  a = "param";
  return arguments[0] + ":" + a;
}
assert.sameValue(aliasDeleted(1), "again:param", "delete severs the mapping");

function aliasAccessor(a) {
  Object.defineProperty(arguments, "0", { get: function () { return "getter"; }, configurable: true });
  a = "param";
  return arguments[0];
}
assert.sameValue(aliasAccessor(1), "getter", "accessor severs the mapping");

function aliasReadonly(a) {
  Object.defineProperty(arguments, "0", { value: "frozen", writable: false });
  a = "param";
  return arguments[0] + ":" + a;
}
assert.sameValue(aliasReadonly(1), "frozen:param", "writable:false severs after value update");

function aliasValueKeepsMapping(a) {
  Object.defineProperty(arguments, "0", { value: "viaDefine" });
  return a;
}
assert.sameValue(aliasValueKeepsMapping(1), "viaDefine", "value-only define keeps the mapping");

function extraArgs(a) {
  arguments[1] = "extra";
  return arguments.length + ":" + arguments[1] + ":" + a;
}
assert.sameValue(extraArgs(1, 2, 3), "3:extra:1", "extra arguments are not mapped");

function missingArgs(a, b) {
  b = "late";
  return arguments.length + ":" + arguments[1] + ":" + (1 in arguments);
}
assert.sameValue(missingArgs(1), "1:undefined:false", "missing arguments are not mapped");

function duplicateNames(a, a) {
  a = "last";
  return arguments[0] + ":" + arguments[1];
}
assert.sameValue(duplicateNames(1, 2), "1:last", "last duplicate parameter is the mapped one");

function lengthWrite() {
  arguments.length = 0;
  return arguments[0] + ":" + arguments.length;
}
assert.sameValue(lengthWrite("keep"), "keep:0", "writing length does not truncate");

function escaping(a, b) {
  return function () { return arguments.length; } && arguments;
}
var escaped = escaping(1, 2);
assert.sameValue(escaped.length, 2, "escaped length");
assert.sameValue(escaped[0], 1, "escaped value");
assert.sameValue(escaped.callee, escaping, "escaped callee");

function escapingMapped(a) {
  return function () { a = "later"; return arguments; };
}
var holder = escapingMapped("first");
function aliasCheck(a) {
  var args = arguments;
  var set = function () { a = "viaClosure"; };
  set();
  return args[0];
}
assert.sameValue(aliasCheck("orig"), "viaClosure", "closure write visible through arguments");

function strictUnmapped(a) {
  "use strict";
  a = "changed";
  return arguments[0];
}
assert.sameValue(strictUnmapped("orig"), "orig", "strict arguments are unmapped");

function defaultsUnmapped(a, b = 2) {
  a = "changed";
  return arguments[0];
}
assert.sameValue(defaultsUnmapped("orig"), "orig", "non-simple parameters are unmapped");

function restUnmapped(a, ...r) {
  a = "changed";
  return arguments[0] + ":" + r.length;
}
assert.sameValue(restUnmapped("orig", 1, 2), "orig:2", "rest parameters are unmapped");

function unmappedShape(a = 0) {
  var d = Object.getOwnPropertyDescriptor(arguments, "callee");
  assert.sameValue(typeof d.get, "function", "callee getter");
  assert.sameValue(d.get, d.set, "callee getter/setter share the thrower");
  assert.sameValue(d.enumerable, false);
  assert.sameValue(d.configurable, false);
  var outerArguments = arguments;
  assert.throws(TypeError, function () { outerArguments.callee; }, "poison pill throws");
  assert.compareArray(Reflect.ownKeys(arguments).map(String), ["0", "length", "callee", "Symbol(Symbol.iterator)"]);
  return d.get;
}
var thrower1 = unmappedShape(10);
var thrower2 = unmappedShape(20);
assert.sameValue(thrower1, thrower2, "poison pill is a single %ThrowTypeError%");

function strictShape() {
  "use strict";
  return Object.getOwnPropertyDescriptor(arguments, "callee").get;
}
assert.sameValue(strictShape(), thrower1, "strict callee uses the same intrinsic");

function noArgs() {
  return Reflect.ownKeys(arguments);
}
var emptyKeys = noArgs();
assert.sameValue(emptyKeys.length, 3, "empty mapped arguments own keys");
assert.sameValue(emptyKeys[0], "length");
assert.sameValue(emptyKeys[1], "callee");
assert.sameValue(emptyKeys[2], Symbol.iterator);
