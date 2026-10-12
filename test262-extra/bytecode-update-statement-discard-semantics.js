/*---
description: >
  An increment/decrement whose result is discarded still performs
  ToNumeric(GetValue(lhs)) and PutValue, so string, BigInt and object operands
  are coerced exactly once, Symbols and const bindings throw, and strict
  unresolvable references throw; the same expression in value context returns
  the prefix/postfix result.
esid: sec-postfix-increment-operator-runtime-semantics-evaluation
flags: [noStrict]
---*/

var valueOfCalls = 0;
globalThis.vo = { valueOf: function () { valueOfCalls++; return 3; } };
globalThis.bigv = 1n;
globalThis.sym = Symbol("s");
const frozenConst = 1;

function stringUpdate() { var s = "5"; s++; return s; }
function bigUpdate() { bigv++; --bigv; bigv++; }
function objectUpdate() { vo++; }
function symbolUpdate() { sym++; }
function constUpdate() { frozenConst++; }
function strictUndeclared() { "use strict"; undeclaredUpdate--; }
function negativeZero() { var z = -0; z++; z--; return z; }
function negativeZeroDown() { var z = -0; z--; return z; }
function nanUpdate() { var n = NaN; n++; return n; }
function valueContext() { var x = 1; var a = x++; var b = ++x; var c = x--; var d = --x; return a * 1000 + b * 100 + c * 10 + d; }
function forUpdate() { var t = 0; for (var i = 0; i < 4; i++) { t += i; } return t * 10 + i; }

assert.sameValue(stringUpdate(), 6, "string operand is converted to a Number");
bigUpdate();
assert.sameValue(bigv, 2n, "BigInt operand stays a BigInt");
objectUpdate();
assert.sameValue(valueOfCalls, 1, "valueOf runs exactly once");
assert.sameValue(vo, 4);
assert.throws(TypeError, symbolUpdate, "ToNumeric(Symbol) throws");
assert.throws(TypeError, constUpdate, "const binding is immutable");
assert.sameValue(frozenConst, 1);
assert.throws(ReferenceError, strictUndeclared, "strict unresolvable reference");
assert.sameValue(negativeZero(), 0, "-0 + 1 - 1 is +0");
assert.sameValue(1 / negativeZero(), Infinity);
assert.sameValue(negativeZeroDown(), -1);
assert.sameValue(nanUpdate(), NaN);
assert.sameValue(valueContext(), 1 * 1000 + 3 * 100 + 3 * 10 + 1);
assert.sameValue(forUpdate(), 64);
