/*---
description: >
  A function declared by eval at global level (a configurable property) that is
  redefined as an accessor is read and written through the accessor.
info: |
  EvalDeclarationInstantiation (sec-evaldeclarationinstantiation) creates the
  binding with CreateGlobalFunctionBinding(vn, fo, true); later references
  use Object Environment Record GetBindingValue and SetMutableBinding.
esid: sec-object-environment-records-setmutablebinding-n-v-s
flags: [noStrict]
---*/

var log = [];
(0, eval)("function ef1346() { return 'fn'; }");
assert.sameValue(ef1346(), "fn");

Object.defineProperty(globalThis, "ef1346", {
  get: function () { return "getter"; },
  set: function (v) { log.push(v); },
  configurable: true
});
ef1346 = "w";
assert.sameValue(log.join(), "w", "setter called");
assert.sameValue(ef1346, "getter", "getter called");

(0, eval)("var ev1346 = 1;");
Object.defineProperty(globalThis, "ev1346", {
  get: function () { return "getter"; },
  set: function (v) { log.push("ev:" + v); },
  configurable: true
});
ev1346 = 2;
assert.sameValue(log.join(), "w,ev:2");
assert.sameValue(ev1346, "getter");
