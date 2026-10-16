/*---
description: Parameter binding of calls into closures that share one function literal
info: |
  10.2.11 FunctionDeclarationInstantiation
  15.1.3 Static Semantics: IsSimpleParameterList

  Every closure of one literal binds the same formal names. A repeated name
  has a single binding that the last argument initializes, a formal named
  `arguments` replaces the arguments object binding, and a call that supplies
  fewer or more arguments than formals binds the missing ones to undefined.
esid: sec-functiondeclarationinstantiation
flags: [noStrict]
---*/

function makeAdder(k) {
  return function add(x, y) {
    return x + y + k;
  };
}
var add1 = makeAdder(1);
var add10 = makeAdder(10);
for (var i = 0; i < 3; i++) {
  assert.sameValue(add1(i, 1), i + 2, "first closure of the literal");
  assert.sameValue(add10(i, 1), i + 11, "second closure of the literal");
}
assert.sameValue(add1(1), NaN, "a missing argument is undefined");
assert.sameValue(add1(1, 2, 3, 4), 4, "extra arguments are ignored");

function dup(a, b, a) {
  return a + ":" + b;
}
var dupAlias = dup;
for (var i = 0; i < 3; i++) {
  assert.sameValue(dup(1, 2, 3), "3:2", "last duplicate wins on every call");
  assert.sameValue(dupAlias(1, 2), "undefined:2", "a missing duplicate shadows an earlier argument");
}

function dupUsesArguments(a, b, a) {
  a = "w";
  return a + ":" + arguments[0] + ":" + arguments[2];
}
assert.sameValue(dupUsesArguments(1, 2, 3), "w:1:w", "only the last duplicate is mapped");
assert.sameValue(dupUsesArguments(1, 2, 3), "w:1:w");

function argumentsParam(arguments) {
  return typeof arguments + ":" + arguments;
}
function argumentsParamUsed(arguments, other) {
  return arguments + other;
}
for (var i = 0; i < 3; i++) {
  assert.sameValue(argumentsParam(5), "number:5");
  assert.sameValue(argumentsParamUsed(1, 2), 3);
}

function argumentsAfterParam(a, arguments) {
  return arguments;
}
assert.sameValue(argumentsAfterParam(1, 2), 2, "the formal replaces the arguments object");

function argumentsFirstThenDup(arguments, arguments) {
  return arguments;
}
assert.sameValue(argumentsFirstThenDup(1, 2), 2);

function evalVar(p) {
  eval("var p = p + 1; var q = 5;");
  return p + ":" + q;
}
for (var i = 0; i < 3; i++) {
  assert.sameValue(evalVar(1), "2:5", "eval redeclaring a formal reuses its binding");
}

function recurse(n, acc) {
  return n === 0 ? acc : recurse(n - 1, acc + n);
}
assert.sameValue(recurse(20, 0), 210);

function shadowsOwnName(shadowsOwnName) {
  return shadowsOwnName;
}
assert.sameValue(shadowsOwnName(7), 7, "a formal shadows the function's own name");

var expr = function self(self) {
  return self;
};
assert.sameValue(expr(3), 3);

var other = $262.createRealm().global;
var inOther = other.eval("(function mk(a, b, a) { return a + ':' + b; })");
for (var i = 0; i < 3; i++) {
  assert.sameValue(inOther(1, 2, 3), "3:2", "same literal evaluated in another realm");
}
var viaThisRealm = (0, eval)("(function mk(a, b, a) { return a + ':' + b; })");
assert.sameValue(viaThisRealm(1, 2, 3), "3:2");

var params = ["a", "b", "a"];
var made = [];
for (var i = 0; i < 3; i++) {
  made.push(new Function("a", "b", "a", "return a + ':' + b + ':' + arguments.length;"));
}
for (var i = 0; i < made.length; i++) {
  assert.sameValue(made[i](1, 2, 3), "3:2:3", "functions from the Function constructor");
}

class Base {
  constructor(x, y) {
    this.sum = x + y;
  }
}
class Derived extends Base {}
for (var i = 0; i < 3; i++) {
  assert.sameValue(new Base(i, 1).sum, i + 1);
  assert.sameValue(new Derived(i, 2).sum, i + 2);
}
