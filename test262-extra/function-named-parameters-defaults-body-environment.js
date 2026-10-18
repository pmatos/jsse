/*---
description: Parameters with defaults or a rest element bind in order, and the body runs in its own var environment
info: |
  10.2.11 FunctionDeclarationInstantiation steps 25-28

  When the formals have parameter expressions the body's var declarations
  live in a separate environment, each initialised from the parameter of the
  same name; defaults are evaluated in the parameter scope left to right with
  every parameter name already in TDZ until its own initialisation.
esid: sec-functiondeclarationinstantiation
flags: [noStrict]
---*/

function defaultSupplied(a, b = 1) { return a + b; }
assert.sameValue(defaultSupplied(1, 5), 6, "supplied value wins over the default");
assert.sameValue(defaultSupplied(1), 2, "omitted argument takes the default");
assert.sameValue(defaultSupplied(1, undefined), 2, "undefined takes the default");
assert.sameValue(defaultSupplied(1, null), 1, "null does not");

function laterDefaultSeesEarlier(a, b = a * 2, c = a + b) { return [a, b, c].join(); }
assert.sameValue(laterDefaultSeesEarlier(1), "1,2,3", "defaults evaluate left to right");
assert.sameValue(laterDefaultSeesEarlier(1, 5), "1,5,6", "a supplied earlier value is seen");

function selfReferenceThrows(a = a) { return a; }
assert.throws(ReferenceError, function () { selfReferenceThrows(); }, "a default may not read its own parameter");
assert.sameValue(selfReferenceThrows(3), 3, "unless the argument is supplied");

function forwardReferenceThrows(a = b, b = 1) { return a; }
assert.throws(ReferenceError, function () { forwardReferenceThrows(); }, "a default may not read a later parameter");

function restParameter(first, ...rest) { return first + ":" + rest.length + ":" + Array.isArray(rest); }
assert.sameValue(restParameter(), "undefined:0:true", "rest of nothing is an empty array");
assert.sameValue(restParameter(1, 2, 3), "1:2:true", "rest collects the remaining arguments");

function restAfterDefault(a = 1, ...more) { return a + ":" + more.join("|"); }
assert.sameValue(restAfterDefault(undefined, 8, 9), "1:8|9", "default and rest together");

function defaultThrows(a = (function () { throw new RangeError("boom"); })()) { return a; }
assert.throws(RangeError, function () { defaultThrows(); }, "an abrupt default propagates");
assert.sameValue(defaultThrows(1), 1, "and is not evaluated when the argument is supplied");

function anonymousFunctionDefaultIsNamed(f = function () {}, g = () => 1, h = class {}) {
  return [f.name, g.name, h.name].join();
}
assert.sameValue(anonymousFunctionDefaultIsNamed(), "f,g,h", "anonymous function defaults take the parameter name");

function bodyVarCopiesParameter(a = 1) {
  var a;
  var seen = a;
  a = 2;
  return seen + ":" + a;
}
assert.sameValue(bodyVarCopiesParameter(), "1:2", "a var of the same name starts as the parameter's value");
assert.sameValue(bodyVarCopiesParameter(7), "7:2", "also for a supplied argument");

function bodyVarShadowsAfterClosure(a = 1, read = function () { return a; }) {
  var a = 5;
  return read() + ":" + a;
}
assert.sameValue(bodyVarShadowsAfterClosure(), "1:5", "a closure in the defaults sees the parameter, not the body var");

function bodyVarWithoutParameter(a = 1) {
  var b;
  b = a + 1;
  return typeof b + b;
}
assert.sameValue(bodyVarWithoutParameter(), "number2", "a plain body var");

function paramNotVisibleToBodyClosureAfterVarWrite(a = 1) {
  var a = 2;
  return function () { return a; };
}
assert.sameValue(paramNotVisibleToBodyClosureAfterVarWrite()(), 2, "body closures see the body var");

function argumentsWithDefaults(a, b = 2) {
  a = 9;
  return arguments.length + ":" + arguments[0] + ":" + a;
}
assert.sameValue(argumentsWithDefaults(1), "1:1:9", "arguments is unmapped when there are defaults");

function arrowDefaults() {
  var f = (a, b = a + 1) => a + b;
  return f(1) + f(1, 1);
}
assert.sameValue(arrowDefaults(), 3 + 2, "arrow functions with defaults");

function recursive(n, step = 1) { return n <= 0 ? 0 : step + recursive(n - step, step); }
assert.sameValue(recursive(5), 5, "recursion creates separate environments");
assert.sameValue(recursive(6, 2), 6, "and keeps its own defaults");

function captured(a = 1) { return function () { return a++; }; }
var c = captured();
assert.sameValue(c() + c(), 3, "closure captures the parameter environment");
var c2 = captured(10);
assert.sameValue(c2(), 10, "a second activation is independent");
assert.sameValue(c(), 3, "the first is unaffected");

function* generatorDefaults(a = 1, b = a + 1) { yield a; yield b; }
assert.sameValue([...generatorDefaults()].join(), "1,2", "generator with defaults");

var thisDefault = { m(a = this.v) { return a; }, v: 4 };
assert.sameValue(thisDefault.m(), 4, "defaults see this");

function evalInDefault(a = eval("1 + 1"), b = a + 1) { return a + b; }
assert.sameValue(evalInDefault(), 5, "direct eval in a default");

function varInBodyEval(a = 1) { eval("var e = a + 1"); return e; }
assert.sameValue(varInBodyEval(), 2, "eval in the body declares into the body environment");

function manyCalls() {
  var t = 0;
  for (var i = 0; i < 50; i++) t += defaultSupplied(i) + Number(restParameter(i, i, i).split(":")[1]);
  return t;
}
assert.sameValue(manyCalls(), 1275 + 100, "repeated calls reuse pooled environments correctly");
