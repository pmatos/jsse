/*---
description: >
  `&&`, `||` and `??` evaluate (and so suspend on) the right operand only when
  the left value does not short-circuit, and yield the left value itself when
  they do.
esid: sec-binary-logical-operators-runtime-semantics-evaluation
info: |
  LogicalORExpression : LogicalORExpression || LogicalANDExpression

  1. Let lref be ? Evaluation of LogicalORExpression.
  2. Let lval be ? GetValue(lref).
  3. If ToBoolean(lval) is true, return lval.
  4. Let rref be ? Evaluation of LogicalANDExpression.
  5. Return ? GetValue(rref).

  LogicalANDExpression && is the mirror image (returns lval when ToBoolean(lval) is false).

  CoalesceExpression : CoalesceExpressionHead ?? BitwiseORExpression

  2. If lval is either undefined or null, then
    a. Let rref be ? Evaluation of BitwiseORExpression.
    b. Return ? GetValue(rref).
  3. Else, return lval.
includes: [compareArray.js]
features: [generators, coalesce-expression, IsHTMLDDA]
---*/

function* nullish(left) {
  var r = left ?? (yield 'right');
  return r;
}

function* or(left) {
  var r = left || (yield 'right');
  return r;
}

function* and(left) {
  var r = left && (yield 'right');
  return r;
}

function* yieldedLeft() {
  var r = (yield 'left') ?? (yield 'right');
  return r;
}

function* leftEvaluatedOnce(counter) {
  var a = counter() && (yield 'right');
  return a;
}

function* boundThroughDeclarations() {
  var v = null ?? (yield 'var');
  let l = 1 && (yield 'let');
  const c = 0 || (yield 'const');
  return [v, l, c];
}

function outcome(it, sent) {
  var first = it.next();
  if (first.done) return ['done', first.value];
  var second = it.next(sent);
  assert.sameValue(second.done, true, 'generator completes after a single resumption');
  return [first.value, second.value];
}

assert.compareArray(outcome(nullish(null), 3), ['right', 3], 'null ?? yield');
assert.compareArray(outcome(nullish(undefined), 3), ['right', 3], 'undefined ?? yield');
assert.compareArray(outcome(nullish(1), 3), ['done', 1], '1 ?? yield does not suspend');
assert.compareArray(outcome(nullish(0), 3), ['done', 0], '0 ?? yield does not suspend');
assert.compareArray(outcome(nullish(''), 3), ['done', ''], "'' ?? yield does not suspend");

assert.compareArray(outcome(or(1), 3), ['done', 1], '1 || yield does not suspend');
assert.compareArray(outcome(or(0), 3), ['right', 3], '0 || yield');

assert.compareArray(outcome(and(0), 3), ['done', 0], '0 && yield does not suspend');
assert.compareArray(outcome(and(1), 3), ['right', 3], '1 && yield');

var dda = $262.IsHTMLDDA;
var ddaResult = outcome(nullish(dda), 3);
assert.sameValue(ddaResult[0], 'done', 'IsHTMLDDA ?? yield does not suspend');
assert.sameValue(ddaResult[1], dda, 'an IsHTMLDDA object is not nullish');

var it = yieldedLeft();
assert.sameValue(it.next().value, 'left', 'first suspension is the left operand');
var res = it.next(null);
assert.sameValue(res.value, 'right', 'nullish sent value evaluates the right operand');
assert.sameValue(it.next(9).value, 9, 'right result becomes the value');

it = yieldedLeft();
it.next();
res = it.next(1);
assert.sameValue(res.done, true, 'non-nullish sent value short-circuits');
assert.sameValue(res.value, 1, 'left result becomes the value');

var calls = 0;
res = leftEvaluatedOnce(function () {
  calls++;
  return 0;
}).next();
assert.sameValue(res.done, true, 'f() && yield does not suspend when f() is falsy');
assert.sameValue(res.value, 0, 'falsy left is the result');
assert.sameValue(calls, 1, 'the left operand is evaluated exactly once');

it = boundThroughDeclarations();
assert.sameValue(it.next().value, 'var', 'var: null ?? yield');
assert.sameValue(it.next('a').value, 'let', 'let: 1 && yield');
assert.sameValue(it.next('b').value, 'const', 'const: 0 || yield');
assert.compareArray(it.next('c').value, ['a', 'b', 'c'], 'declarations bind the yielded values');
