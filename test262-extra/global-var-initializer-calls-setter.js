/*---
description: >
  A global var initializer for a name that is already an accessor property of
  the global object calls the setter; the declaration itself does not redefine
  the property.
info: |
  EvalDeclarationInstantiation (sec-evaldeclarationinstantiation) does not
  touch an existing property (CreateGlobalVarBinding is a no-op); the
  initializer's PutValue (sec-putvalue) reaches SetMutableBinding on the object
  record, which calls [[Set]].
esid: sec-evaldeclarationinstantiation
flags: [noStrict]
---*/

var log = [];
Object.defineProperty(globalThis, "vi1346", {
  get: function () { return "getter"; },
  set: function (v) { log.push(v); },
  configurable: true
});

(0, eval)("var vi1346 = 5;");
assert.sameValue(log.join(), "5", "initializer calls the setter");
assert.sameValue(vi1346, "getter");
assert.sameValue(typeof Object.getOwnPropertyDescriptor(globalThis, "vi1346").set, "function");
