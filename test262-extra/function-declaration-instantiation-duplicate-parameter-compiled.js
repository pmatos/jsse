/*---
description: Duplicate parameter names and var/parameter name collisions in locally resolved function bodies
info: |
  10.2.11 FunctionDeclarationInstantiation
  15.1.3 Static Semantics: IsSimpleParameterList

  With duplicate formals the last one initializes the single binding. A `var`
  with the same name as a parameter reuses that binding and keeps its value
  until it is assigned; `var`s are otherwise initialized to undefined.
esid: sec-functiondeclarationinstantiation
flags: [noStrict]
---*/

function dup(a, b, a) {
  var out = a;
  a = a + 1;
  return out + ":" + a + ":" + b;
}
assert.sameValue(dup(1, 2, 3), "3:4:2", "last duplicate wins and is a single binding");
assert.sameValue(dup(1, 2), "undefined:NaN:2", "the later duplicate shadows an earlier argument");

function varShadowsParam(p) {
  var p;
  var q;
  var before = p;
  var qBefore = q;
  p = p * 2;
  return [before, qBefore, p].join();
}
assert.sameValue(varShadowsParam(21), "21,,42");

function varBeforeUse() {
  var early = later;
  var later = 5;
  return early + "," + later;
}
assert.sameValue(varBeforeUse(), "undefined,5", "var is hoisted and undefined before assignment");

function manyLocals(a0, a1, a2) {
  var b0 = 0, b1 = 1, b2 = 2, b3 = 3, b4 = 4, b5 = 5, b6 = 6, b7 = 7;
  var b8 = 8, b9 = 9, b10 = 10, b11 = 11, b12 = 12, b13 = 13, b14 = 14, b15 = 15;
  var b16 = 16, b17 = 17, b18 = 18, b19 = 19;
  b19 += a2;
  b0++;
  return a0 + a1 + b0 + b9 + b19;
}
assert.sameValue(manyLocals(1, 2, 3), 1 + 2 + 1 + 9 + 22, "more locals than the scan threshold");

var calls = 0;
function recur(n) {
  var local = n;
  calls++;
  if (n > 0) {
    recur(n - 1);
  }
  return local;
}
assert.sameValue(recur(5), 5, "an inner activation does not disturb the outer local");
assert.sameValue(calls, 6);

function paramNamedArguments(arguments) {
  var x = arguments;
  arguments = 7;
  return x + ":" + arguments;
}
assert.sameValue(paramNamedArguments(3), "3:7");

function varNamedArguments() {
  var arguments = 4;
  arguments++;
  return arguments;
}
assert.sameValue(varNamedArguments(1, 2), 5);

function readsArguments(a) {
  return arguments.length + a;
}
assert.sameValue(readsArguments(10, 20, 30), 13);
