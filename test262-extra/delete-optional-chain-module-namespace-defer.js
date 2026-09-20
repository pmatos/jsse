/*---
description: >
  `delete` through an optional chain on a deferred module namespace triggers
  evaluation for non-symbol-like keys, like the plain member form.
info: |
  IsSymbolLikeNamespaceKey ( P, O )
    1. If P is a Symbol, return true.
    2. If O.[[Deferred]] is true and P is "then", return true.
    3. Return false.

  [[Delete]] ( P )
    1. If IsSymbolLikeNamespaceKey(P, O), return ! OrdinaryDelete(O, P).
    2. Let exports be ? GetModuleExportsList(O).
    3. ...

  GetModuleExportsList performs EvaluateSync for a deferred namespace.
esid: sec-module-namespace-exotic-objects-delete-p
features: [import-defer]
flags: [module]
---*/

import "./delete-optional-chain-module-namespace-defer-setup_FIXTURE.mjs";
import defer * as ns from "./delete-optional-chain-module-namespace-defer_FIXTURE.mjs";

assert.sameValue(globalThis.evaluations.length, 0, "import defer does not trigger evaluation");

assert.sameValue(delete ns?.[Symbol.iterator], true, "optional chain: absent symbol key");
assert.sameValue(globalThis.evaluations.length, 0, "optional chain: symbol key does not trigger evaluation");

assert.throws(TypeError, function () {
  delete ns?.then;
}, "optional chain: exported `then` is an ordinary non-configurable property");
assert.sameValue(globalThis.evaluations.length, 0, "optional chain: `then` does not trigger evaluation");

assert.throws(TypeError, function () {
  delete ns?.exported;
}, "optional chain: exported name is not deletable");
assert.sameValue(globalThis.evaluations.length, 1, "optional chain: exported name triggers evaluation");
assert.sameValue(globalThis.evaluations[0], "dep", "the dependency was evaluated");
