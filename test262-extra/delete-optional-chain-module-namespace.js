/*---
description: >
  `delete` through an optional chain applies the module namespace exotic
  object's [[Delete]] exactly like the plain member form.
info: |
  Module Namespace Exotic Objects [[Delete]] ( P )

  1. If P is a Symbol, then
    a. Return ! OrdinaryDelete(O, P).
  2. Let exports be O.[[Exports]].
  3. If exports contains P, return false.
  4. Return true.

  Runtime Semantics: Evaluation
  UnaryExpression : delete UnaryExpression

  5.f. If deleteStatus is false and ref.[[Strict]] is true, throw a TypeError
       exception.

  Module code is always strict.
esid: sec-module-namespace-exotic-objects-delete-p
flags: [module]
---*/

import * as ns from "./delete-optional-chain-module-namespace_FIXTURE.mjs";

assert.throws(TypeError, function () {
  delete ns.exported;
}, "plain: exported name");

assert.throws(TypeError, function () {
  delete ns?.exported;
}, "optional chain: exported name");

assert.throws(TypeError, function () {
  delete ns?.["exported"];
}, "optional chain, computed: exported name");

assert.throws(TypeError, function () {
  var holder = { ns: ns };
  delete holder?.ns.exported;
}, "nested optional chain: exported name");

assert.throws(TypeError, function () {
  var holder = { ns: ns };
  delete holder?.ns?.["fn"];
}, "nested optional chain, computed: exported function");

assert.sameValue(ns.exported, "exported", "exported binding intact");
assert.sameValue(typeof ns.fn, "function", "exported function binding intact");

assert.sameValue(delete ns.notExported, true, "plain: non-exported name");
assert.sameValue(delete ns?.notExported, true, "optional chain: non-exported name");
assert.sameValue(delete ns?.["notExported"], true, "optional chain, computed: non-exported name");

assert.throws(TypeError, function () {
  delete ns[Symbol.toStringTag];
}, "plain: @@toStringTag is non-configurable");
assert.throws(TypeError, function () {
  delete ns?.[Symbol.toStringTag];
}, "optional chain: @@toStringTag is non-configurable");

assert.sameValue(delete ns[Symbol.iterator], true, "plain: absent symbol");
assert.sameValue(delete ns?.[Symbol.iterator], true, "optional chain: absent symbol");

assert.sameValue(ns[Symbol.toStringTag], "Module", "@@toStringTag intact");
