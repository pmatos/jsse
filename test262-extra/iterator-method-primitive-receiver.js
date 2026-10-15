// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-getiterator
description: >
  GetIterator reads @@iterator with GetV, so an inherited accessor on
  String.prototype is called with the string primitive as its receiver in
  array binding and assignment patterns, spread and for-of.
info: |
  GetIterator ( obj, kind )

  1. If kind is async, then
     ...
  2. Else,
     a. Let method be ? GetMethod(obj, @@iterator).
  ...

  GetMethod ( V, P )

  1. Let func be ? GetV(V, P).
  2. If func is either undefined or null, return undefined.
  ...

  GetV ( V, P )

  1. Let O be ? ToObject(V).
  2. Return ? O.[[Get]](P, V).
features: [Symbol.iterator, destructuring-binding, destructuring-assignment, generators]
---*/

var receivers = [];
Object.defineProperty(String.prototype, Symbol.iterator, {
  get: function () {
    "use strict";
    receivers.push(typeof this);
    return function* () { yield "x"; };
  },
  configurable: true
});

function check(label, run) {
  receivers.length = 0;
  run();
  assert.sameValue(receivers.length, 1, label + ": getter calls");
  assert.sameValue(receivers[0], "string", label + ": receiver");
}

check("array declaration pattern", function () {
  const [a] = "abc";
  assert.sameValue(a, "x");
});

check("array assignment pattern", function () {
  var a;
  [a] = "abc";
  assert.sameValue(a, "x");
});

check("function parameter pattern", function () {
  (function ([a]) { assert.sameValue(a, "x"); })("abc");
});

check("spread in array literal", function () {
  var spread = [..."abc"];
  assert.sameValue(spread.length, 1);
});

check("spread in call", function () {
  var count = (function () { return arguments.length; })(..."abc");
  assert.sameValue(count, 1);
});

check("for-of", function () {
  var count = 0;
  for (var c of "abc") count++;
  assert.sameValue(count, 1);
});

check("yield*", function () {
  function* g() { yield* "abc"; }
  assert.sameValue([...g()].length, 1);
});

check("Array.from", function () {
  assert.sameValue(Array.from("abc").length, 1);
});

delete String.prototype[Symbol.iterator];
assert.throws(TypeError, function () {
  const [a] = "abc";
}, "no @@iterator on a string");

Object.defineProperty(String.prototype, Symbol.iterator, {
  get: function () { return null; },
  configurable: true
});
assert.throws(TypeError, function () {
  const [a] = "abc";
}, "null @@iterator on a string");

assert.throws(TypeError, function () {
  const [a] = undefined;
}, "undefined is not iterable");
assert.throws(TypeError, function () {
  var a;
  [a] = null;
}, "null is not iterable");
