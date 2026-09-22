/*---
description: >
  An assignment whose target reference and value both contain `yield`
  evaluates the target's base, then its key, then the right-hand side, and
  finally stores through the target reference.
esid: sec-assignment-operators-runtime-semantics-evaluation
info: |
  AssignmentExpression : LeftHandSideExpression = AssignmentExpression

  1. If LeftHandSideExpression is neither an ObjectLiteral nor an ArrayLiteral, then
    a. Let lref be ? Evaluation of LeftHandSideExpression.
    ...
    c. Let rref be ? Evaluation of AssignmentExpression.
    d. Let rval be ? GetValue(rref).
    e. Perform ? PutValue(lref, rval).
    f. Return rval.

  EvaluatePropertyAccessWithExpressionKey (sec-evaluate-property-access-with-expression-key)
  evaluates the base expression, then the key expression, and defers
  ToPropertyKey.
includes: [compareArray.js]
features: [generators]
---*/

function* keyAndValue(o) {
  var r = (o[yield 'key'] = yield 'value');
  return r;
}

function* keyOnly(o) {
  o[yield 'key'] = 5;
}

function* compound(o) {
  o[yield 'key'] += yield 'value';
}

function* nestedKeys(o) {
  o[yield 'a'][yield 'b'] = yield 'value';
}

function* yieldedBase() {
  (yield 'base').k = yield 'value';
}

function* baseBeforeYields(g) {
  g()[yield 'key'] = yield 'value';
}

var o = {};
var it = keyAndValue(o);
assert.sameValue(it.next().value, 'key', 'first suspension is the key');
assert.sameValue(it.next('k').value, 'value', 'second suspension is the value');
var res = it.next(5);
assert.sameValue(res.done, true, 'completes');
assert.sameValue(res.value, 5, 'assignment expression value');
assert.sameValue(o.k, 5, 'o[yield k] = yield v stores through the target');

o = {};
it = keyOnly(o);
it.next();
it.next('k');
assert.sameValue(o.k, 5, 'o[yield k] = v');

o = { k: 1 };
it = compound(o);
it.next();
it.next('k');
it.next(5);
assert.sameValue(o.k, 6, 'o[yield k] += yield v');

o = { a: { b: 0 } };
it = nestedKeys(o);
it.next();
it.next('a');
it.next('b');
it.next(7);
assert.sameValue(o.a.b, 7, 'o[yield a][yield b] = yield c');

o = {};
it = yieldedBase();
it.next();
it.next(o);
it.next(3);
assert.sameValue(o.k, 3, '(yield).k = yield v');

var log = [];
o = {};
it = baseBeforeYields(function () {
  log.push('g');
  return o;
});
it.next();
assert.compareArray(log, ['g'], 'the base expression is evaluated before the first yield');
it.next('k');
it.next(5);
assert.sameValue(o.k, 5, 'g()[yield k] = yield v stores through g()');
