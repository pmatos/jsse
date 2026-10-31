/*---
description: >
  Evaluating a block-level function declaration in global eval code (Annex B)
  assigns the function to the global binding with [[Set]], so an existing
  accessor property receives it through its setter.
info: |
  B.3.2.3 Changes to EvalDeclarationInstantiation leaves an existing property
  untouched (CreateGlobalVarBinding is a no-op); evaluating the declaration
  performs genv.SetMutableBinding(F, fobj, false), which calls [[Set]] on the
  global object (sec-object-environment-records-setmutablebinding-n-v-s).
esid: sec-web-compat-evaldeclarationinstantiation
flags: [noStrict]
---*/

var received = [];
Object.defineProperty(globalThis, "bf1346", {
  get: function () { return "getter"; },
  set: function (v) { received.push(v); },
  configurable: true
});

(0, eval)('{ function bf1346() { return "block"; } }');

assert.sameValue(received.length, 1, "setter called once");
assert.sameValue(typeof received[0], "function");
assert.sameValue(received[0](), "block");
assert.sameValue(bf1346, "getter");
