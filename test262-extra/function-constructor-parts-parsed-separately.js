// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-createdynamicfunction
description: >
  CreateDynamicFunction parses the parameters as FormalParameters and the body
  as a FunctionBody on their own, so neither part can close or reopen the
  synthetic function wrapper that the combined source text is parsed under.
info: |
  20.2.1.1.1 CreateDynamicFunction ( constructor, newTarget, kind, parameterArgs, bodyArg )

  ...
  17. Let parameters be ParseText(P, parameterSym).
  18. If parameters is a List of errors, throw a SyntaxError exception.
  19. Let body be ParseText(bodyParseString, bodySym).
  20. If body is a List of errors, throw a SyntaxError exception.
features: [generators, async-functions, async-iteration]
---*/

var GeneratorFunction = Object.getPrototypeOf(function* () {}).constructor;
var AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
var AsyncGeneratorFunction = Object.getPrototypeOf(async function* () {}).constructor;

var constructors = {
  Function: Function,
  GeneratorFunction: GeneratorFunction,
  AsyncFunction: AsyncFunction,
  AsyncGeneratorFunction: AsyncGeneratorFunction
};

var leaked = 0;
globalThis.leak = function () { leaked++; };

Object.keys(constructors).forEach(function (name) {
  var C = constructors[name];

  assert.throws(SyntaxError, function () {
    C("}); leak(); (function(){");
  }, name + ": body closes the wrapper and runs a statement");

  assert.throws(SyntaxError, function () {
    C("exports", "}); leak(); (function(){");
  }, name + ": body closes the wrapper after a parameter");

  assert.throws(SyntaxError, function () {
    C("}) , (function(){");
  }, name + ": body closes the wrapper into a sequence expression");

  assert.throws(SyntaxError, function () {
    C("} leak(); {");
  }, name + ": unbalanced closing brace followed by a block");

  assert.throws(SyntaxError, function () {
    C("}");
  }, name + ": lone closing brace");

  assert.throws(SyntaxError, function () {
    C("a) {}); leak(); (function(b", "");
  }, name + ": parameters close the parameter list and wrapper");

  assert.throws(SyntaxError, function () {
    C("a) {}, function(b", "");
  }, name + ": parameters reopen a second function");

  assert.throws(SyntaxError, function () {
    C("a", "b) { leak(); } function(c", "");
  }, name + ": parameter string split across arguments");

  var f = C("a", "b", "return 1;");
  assert.sameValue(typeof f, "function", name + ": well-formed parts still create a function");
  assert.sameValue(f.length, 2, name + ": parameter count");
});

assert.sameValue(leaked, 0, "no injected statement ever ran");

var f = Function("a, b = function () { return 1; }", "{ let x = 1; } return a + b();");
assert.sameValue(f(2), 3, "braces and nested functions balanced inside the parts are accepted");

assert.sameValue(Function("a //", "return a;")(5), 5, "trailing line comment in parameters");
assert.sameValue(Function("return 7; // trailing")(), 7, "trailing line comment in body");
assert.sameValue(Function("return new.target === undefined;")(), true, "new.target in the body");

var gen = GeneratorFunction("a = 1", "yield a; yield* [2];");
assert.sameValue(gen().next().value, 1, "yield in a GeneratorFunction body");
assert.sameValue(typeof AsyncFunction("a", "return await a;"), "function", "await in an AsyncFunction body");
assert.sameValue(typeof AsyncGeneratorFunction("a", "yield await a;"), "function", "yield and await in an AsyncGeneratorFunction body");

assert.throws(SyntaxError, function () {
  GeneratorFunction("a = yield", "");
}, "yield expression in GeneratorFunction parameters");

assert.throws(SyntaxError, function () {
  AsyncFunction("a = await 1", "");
}, "await expression in AsyncFunction parameters");

assert.throws(SyntaxError, function () {
  Function("super.x;");
}, "super property is not allowed in the body");
