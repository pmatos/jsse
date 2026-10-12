// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-functiondeclarationinstantiation
description: >
  FunctionDeclarationInstantiation steps 15-22: whether an `arguments` binding
  exists must be unobservable except through the cases the spec lists — every
  reference, declaration, parameter, nested arrow and direct eval still sees
  the right `arguments` — while a function that never mentions `arguments`
  keeps working with the legacy `fn.arguments` accessor.
info: |
  10.2.11 FunctionDeclarationInstantiation ( func, argumentsList )
    15. Let argumentsObjectNeeded be true.
    16. If func.[[ThisMode]] is lexical, argumentsObjectNeeded = false.
    17. Else if parameterNames contains "arguments", argumentsObjectNeeded = false.
    18. Else if hasParameterExpressions is false, then
        a. If functionNames contains "arguments" or lexicalNames contains
           "arguments", argumentsObjectNeeded = false.
flags: [noStrict]
---*/

function declaredFunction() {
  return typeof arguments;
  function arguments() {}
}
assert.sameValue(declaredFunction(1, 2), "function", "function declaration shadows");

function varOnly() {
  var arguments;
  return typeof arguments;
}
assert.sameValue(varOnly(1, 2), "object", "var arguments keeps the object");

function varWithInit() {
  var arguments = 3;
  return arguments;
}
assert.sameValue(varWithInit(7), 3, "var initializer overwrites");

function paramNamed(arguments) {
  return arguments;
}
assert.sameValue(paramNamed(9), 9, "parameter named arguments");

function viaArrow() {
  return (() => arguments.length)();
}
assert.sameValue(viaArrow(1, 2, 3), 3, "arrow reads the enclosing arguments");

function viaEvalInArrow() {
  return (() => eval("arguments.length"))();
}
assert.sameValue(viaEvalInArrow(1, 2), 2, "direct eval in arrow");

function viaWith() {
  var o = { arguments: 5 };
  with (o) {
    return arguments;
  }
}
assert.sameValue(viaWith(1), 5, "with scope shadows arguments");

function defaultParamVar(a = 1) {
  var arguments;
  return typeof arguments;
}
assert.sameValue(defaultParamVar(), "object", "var arguments with parameter expressions");

function defaultParamVarInit(a = 1) {
  var arguments = 3;
  return arguments;
}
assert.sameValue(defaultParamVarInit(7), 3, "var arguments initializer with parameter expressions");

function strictVariant() {
  "use strict";
  return typeof arguments;
}
assert.sameValue(strictVariant(), "object", "strict mode arguments");

function innerOwnArguments() {
  return (function () { return arguments.length; })(1, 2, 3, 4);
}
assert.sameValue(innerOwnArguments(1), 4, "nested function has its own arguments");

function curried() {
  return () => () => arguments[0];
}
assert.sameValue(curried("q")()(), "q", "arrow chain");

function aliasing(x, y) {
  arguments[0] = 10;
  return x;
}
assert.sameValue(aliasing(1, 2), 10, "mapped arguments alias parameters");

function duplicate(a, a) {
  return arguments.length + ":" + a;
}
assert.sameValue(duplicate(1, 2), "2:2", "duplicate parameter names");

function blockFunction() {
  {
    function arguments() {}
  }
  return 1;
}
blockFunction(1);
assert.sameValue("arguments" in globalThis, false, "Annex B block function does not leak globally");

// Legacy fn.arguments for functions that never mention `arguments`.
function noMention(a, b) {
  return noMention.arguments.length;
}
assert.sameValue(noMention(1), 1, "fn.arguments while active");
assert.sameValue(noMention.arguments, null, "fn.arguments when inactive");

function outerNoMention() {
  return peek();
}
function peek() {
  return outerNoMention.arguments === null
    ? "null"
    : outerNoMention.arguments.length + ":" + outerNoMention.arguments[7];
}
// Exercises argument counts around any small inline-storage boundary.
for (var n = 0; n <= 9; n++) {
  var args = [];
  for (var i = 0; i < n; i++) args.push(i * 2);
  var expected = n === 0 ? "0:undefined" : n + ":" + (n > 7 ? 14 : undefined);
  assert.sameValue(outerNoMention.apply(null, args), expected, "fn.arguments with " + n + " args");
}

// Non-simple parameters called repeatedly with differing arguments.
function nonSimple(a, b = a + 1) {
  var a;
  var arguments;
  var seen = typeof arguments;
  var c = () => a + b;
  a = a + 100;
  return seen + ":" + c();
}
assert.sameValue(nonSimple(1), "object:103", "non-simple first call");
assert.sameValue(nonSimple(5, 6), "object:111", "non-simple second call");
assert.sameValue(nonSimple(1), "object:103", "non-simple third call reuses analysis");
