/*---
description: >
  An assignment whose right-hand side contains `yield` still evaluates the
  target reference (base, then key) and, for compound assignment, reads the
  target's current value before the right-hand side runs. Logical assignments
  evaluate (and so suspend on) the right-hand side only when the target's
  value does not short-circuit.
esid: sec-assignment-operators-runtime-semantics-evaluation
info: |
  AssignmentExpression : LeftHandSideExpression AssignmentOperator AssignmentExpression

  1. Let lref be ? Evaluation of LeftHandSideExpression.
  2. Let lval be ? GetValue(lref).
  3. Let rref be ? Evaluation of AssignmentExpression.
  4. Let rval be ? GetValue(rref).
  5. Let assignmentOpText be the source text matched by AssignmentOperator.
  6. Let opText be the sequence of Unicode code points associated with assignmentOpText in the following table:
  7. Let r be ? ApplyStringOrNumericBinaryOperator(lval, opText, rval).
  8. Perform ? PutValue(lref, r).

  AssignmentExpression : LeftHandSideExpression &&= AssignmentExpression

  1. Let lref be ? Evaluation of LeftHandSideExpression.
  2. Let lval be ? GetValue(lref).
  3. If ToBoolean(lval) is false, return lval.
  4. Let rref be ? Evaluation of AssignmentExpression.
  5. Let rval be ? GetValue(rref).
  6. Perform ? PutValue(lref, rval).

  The `||=` and `??=` forms short-circuit on a true / non-nullish lval respectively.
includes: [compareArray.js]
features: [generators, logical-assignment-operators, coalesce-expression]
---*/

var x = 1;
function* compoundIdentifier() {
  x += yield 'rhs';
  return x;
}
var it = compoundIdentifier();
assert.sameValue(it.next().value, 'rhs', 'suspends on the right-hand side');
x = 10;
assert.sameValue(it.next(5).value, 6, 'x += yield reads x before suspending');
assert.sameValue(x, 6, 'x holds the sum');

var o = { k: 1 };
function* compoundMember() {
  o.k += yield 'rhs';
  o['k'] *= yield 'rhs';
}
it = compoundMember();
it.next();
o.k = 10;
it.next(5);
assert.sameValue(o.k, 6, 'o.k += yield reads o.k before suspending');
o.k = 99;
it.next(2);
assert.sameValue(o.k, 12, 'o[k] *= yield reads o[k] before suspending');

function* shortCircuits(target) {
  var a = 1;
  a ||= yield 'a';
  var b = 0;
  b &&= yield 'b';
  var c = 0;
  c ??= yield 'c';
  target.p ||= yield 'p';
  target.q &&= yield 'q';
  target.r ??= yield 'r';
  target[yield 'key'] ||= yield 'keyed';
  return [a, b, c];
}
var target = { p: 1, q: 0, r: 0, s: 1 };
it = shortCircuits(target);
var res = it.next();
assert.sameValue(res.value, 'key', 'no right-hand side is evaluated before the awaited key');
res = it.next('s');
assert.sameValue(res.done, true, 'every short-circuiting right-hand side is skipped');
assert.compareArray(res.value, [1, 0, 0], 'locals keep their values');
assert.sameValue(target.s, 1, 'keyed target keeps its value');

function* evaluatesRight(t) {
  var a = 0;
  a ||= yield 'a';
  var b = 1;
  b &&= yield 'b';
  var c = null;
  c ??= yield 'c';
  t.p ||= yield 'p';
  var result = (t.q ||= yield 'q');
  return [a, b, c, result];
}
var t = { p: 0, q: undefined };
it = evaluatesRight(t);
assert.sameValue(it.next().value, 'a', 'a ||= yield');
assert.sameValue(it.next(2).value, 'b', 'b &&= yield');
assert.sameValue(it.next(3).value, 'c', 'c ??= yield');
assert.sameValue(it.next(4).value, 'p', 't.p ||= yield');
assert.sameValue(it.next(5).value, 'q', 't.q ||= yield');
res = it.next(6);
assert.compareArray(res.value, [2, 3, 4, 6], 'values assigned and the expression result');
assert.sameValue(t.p, 5, 't.p assigned');
assert.sameValue(t.q, 6, 't.q assigned');

var log = [];
var base = {};
function* targetBeforeValue() {
  (log.push('base'), base)[(log.push('key'), 'k')] = yield 'value';
}
it = targetBeforeValue();
it.next();
assert.compareArray(log, ['base', 'key'], 'base and key are evaluated before the first yield');
it.next(1);
assert.sameValue(base.k, 1, 'value is stored through the target');

var o1 = {};
var o2 = {};
var current = o1;
function* baseCaptured() {
  current.k = yield 'value';
}
it = baseCaptured();
it.next();
current = o2;
it.next(1);
assert.sameValue(o1.k, 1, 'the base is captured before the yield');
assert.sameValue(o2.k, undefined, 'the later base is untouched');
