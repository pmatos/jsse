// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-runtime-semantics-keyedbindinginitialization
description: >
  An object binding or assignment pattern reads each property with GetV, so
  an inherited accessor on a primitive's prototype is called with the
  primitive itself as its receiver, not the ToObject wrapper.
info: |
  KeyedBindingInitialization : BindingElement : SingleNameBinding

  ...
  3. Let v be ? GetV(value, propertyName).

  KeyedDestructuringAssignmentEvaluation : AssignmentElement : DestructuringAssignmentTarget Initializer_opt

  ...
  3. Let v be ? GetV(value, propertyName).

  GetV ( V, P )

  1. Let O be ? ToObject(V).
  2. Return ? O.[[Get]](P, V).

  CopyDataProperties ( target, source, excludedItems )

  ...
  3. Let from be ! ToObject(source).
  ...
  c. Let propValue be ? Get(from, nextKey).
flags: [noStrict]
features: [Symbol, BigInt, destructuring-binding, destructuring-assignment]
---*/

var seen = [];
function getter() {
  "use strict";
  seen.push(typeof this);
  return typeof this;
}

var cases = [
  { proto: String.prototype, value: "a", type: "string" },
  { proto: Number.prototype, value: 1, type: "number" },
  { proto: Boolean.prototype, value: true, type: "boolean" },
  { proto: Symbol.prototype, value: Symbol("s"), type: "symbol" },
  { proto: BigInt.prototype, value: 1n, type: "bigint" }
];

var keyName = "gx";
cases.forEach(function (c) {
  Object.defineProperty(c.proto, "gx", { get: getter, configurable: true });
  Object.defineProperty(c.proto, "gs", {
    get: function () { return typeof this; },
    configurable: true
  });
});

function check(c, label, run) {
  seen.length = 0;
  var result = run(c.value);
  assert.sameValue(result, c.type, c.type + " " + label + ": result");
  assert.sameValue(seen.length, 1, c.type + " " + label + ": getter calls");
  assert.sameValue(seen[0], c.type, c.type + " " + label + ": receiver");
}

cases.forEach(function (c) {
  check(c, "declaration shorthand", function (v) {
    const { gx } = v;
    return gx;
  });

  check(c, "declaration key: binding", function (v) {
    const { gx: out } = v;
    return out;
  });

  check(c, "declaration computed key", function (v) {
    const { [keyName]: out } = v;
    return out;
  });

  check(c, "declaration string key", function (v) {
    const { "gx": out } = v;
    return out;
  });

  check(c, "declaration with initializer", function (v) {
    const { gx = "unused" } = v;
    return gx;
  });

  check(c, "declaration with rest", function (v) {
    const { gx, ...rest } = v;
    return gx;
  });

  check(c, "var declaration", function (v) {
    var { gx: out } = v;
    return out;
  });

  check(c, "nested pattern", function (v) {
    const [{ gx: out }] = [v];
    return out;
  });

  check(c, "function parameter", function (v) {
    return (function ({ gx }) { return gx; })(v);
  });

  check(c, "function parameter with initializer", function (v) {
    return (function ({ gx } = {}) { return gx; })(v);
  });

  check(c, "arrow parameter", function (v) {
    return (({ gx }) => gx)(v);
  });

  check(c, "assignment pattern", function (v) {
    var out;
    ({ gx: out } = v);
    return out;
  });

  check(c, "assignment pattern computed key", function (v) {
    var out;
    ({ [keyName]: out } = v);
    return out;
  });

  check(c, "assignment pattern with initializer", function (v) {
    var out;
    ({ gx: out = "unused" } = v);
    return out;
  });

  check(c, "assignment pattern with rest", function (v) {
    var out, rest;
    ({ gx: out, ...rest } = v);
    return out;
  });

  check(c, "assignment to member target", function (v) {
    var target = {};
    ({ gx: target.out } = v);
    return target.out;
  });

  check(c, "for-of const head", function (v) {
    var out;
    for (const { gx } of [v]) out = gx;
    return out;
  });

  check(c, "for-of var head", function (v) {
    var out;
    for (var { gx: got } of [v]) out = got;
    return out;
  });

  check(c, "for-of assignment head", function (v) {
    var out;
    for ({ gx: out } of [v]) ;
    return out;
  });

  check(c, "catch parameter", function (v) {
    try {
      throw v;
    } catch ({ gx }) {
      return gx;
    }
  });

  check(c, "Reflect.get control", function (v) {
    return Reflect.get(c.proto, "gx", v);
  });

  check(c, "member access control", function (v) {
    return v.gx;
  });

  assert.sameValue(
    (function ({ gs }) { return gs; })(c.value),
    "object",
    c.type + " sloppy getter still sees the wrapper"
  );
  var sloppyOut;
  ({ gs: sloppyOut } = c.value);
  assert.sameValue(sloppyOut, "object", c.type + " sloppy getter in assignment pattern");
});

var { 0: first, ...restOut } = "ab";
assert.sameValue(first, "a", "string index key before rest");
assert.sameValue(restOut[1], "b", "string rest copies the remaining indexed own properties");

assert.throws(TypeError, function () {
  const { a } = null;
}, "null declaration pattern");
assert.throws(TypeError, function () {
  const { a } = undefined;
}, "undefined declaration pattern");
assert.throws(TypeError, function () {
  const {} = undefined;
}, "empty pattern with undefined");
assert.throws(TypeError, function () {
  const {} = null;
}, "empty pattern with null");
assert.throws(TypeError, function () {
  var a;
  ({ a } = null);
}, "null assignment pattern");
assert.throws(TypeError, function () {
  var a;
  ({} = undefined);
}, "empty assignment pattern with undefined");
