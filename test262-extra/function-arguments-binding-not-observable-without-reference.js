/*---
description: The arguments binding is only observable through an identifier reference, direct eval, or a declaration of that name
info: |
  10.2.11 FunctionDeclarationInstantiation steps 15-22 (argumentsObjectNeeded)
  10.4.4 Arguments Exotic Objects

  An implementation may omit the arguments binding of a function that never
  references `arguments`; every observable use must still see the object.
esid: sec-functiondeclarationinstantiation
flags: [noStrict]
---*/

function declaredVar() {
  var arguments;
  return typeof arguments;
}
assert.sameValue(declaredVar(), "object", "var arguments keeps the arguments object");

function assignedVar() {
  var arguments = 1;
  return arguments;
}
assert.sameValue(assignedVar(), 1);
assert.sameValue("arguments" in globalThis, false, "no stray global arguments binding");

function paramNamed(arguments) {
  return arguments;
}
assert.sameValue(paramNamed(42), 42, "a parameter named arguments wins");

function viaArrow() {
  return (() => arguments.length)();
}
assert.sameValue(viaArrow(1, 2, 3), 3, "arrow reads the enclosing arguments");

function viaNestedArrow() {
  return (() => () => arguments[0])()();
}
assert.sameValue(viaNestedArrow("x"), "x");

function viaEval() {
  return eval("arguments.length");
}
assert.sameValue(viaEval(1, 2), 2, "direct eval sees arguments");

function viaWith() {
  with ({}) {
    return arguments.length;
  }
}
assert.sameValue(viaWith(1), 1, "with-body identifier resolves to arguments");

function neverNames() {
  return neverNames.arguments;
}
var legacy = neverNames(5, 6);
assert.sameValue(legacy === null || legacy.length === 2, true, "legacy f.arguments tolerates an omitted binding");

function shadowedByFunction() {
  function arguments() {}
  return typeof arguments;
}
assert.sameValue(shadowedByFunction(), "function", "function declaration named arguments");

function plain(a) {
  return typeof arguments;
}
assert.sameValue(plain(1), "object");

function noRef(a) {
  return a;
}
for (var i = 0; i < 20; i++) {
  assert.sameValue(noRef(i), i);
}
assert.sameValue(typeof arguments, "undefined", "global code has no arguments binding");
