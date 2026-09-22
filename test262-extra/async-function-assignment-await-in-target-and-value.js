/*---
description: >
  An assignment whose target reference and value both contain `await`
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
flags: [async]
includes: [compareArray.js]
features: [async-functions]
---*/

async function awaitedKeyAndValue() {
  var o = {};
  var r = (o[await 'k'] = await 5);
  return [r, o.k];
}

async function awaitedKeyOnly() {
  var o = {};
  o[await 'k'] = 5;
  return o.k;
}

async function awaitedValueOnly() {
  var o = {};
  o.k = await 5;
  return o.k;
}

async function compoundAssignment() {
  var o = { k: 1 };
  o[await 'k'] += await 5;
  return o.k;
}

async function nestedKeys() {
  var o = { a: { b: 0 } };
  o[await 'a'][await 'b'] = await 7;
  return o.a.b;
}

async function awaitedBase() {
  var o = {};
  (await o).k = await 3;
  return o.k;
}

async function baseEvaluatedBeforeAwaits() {
  var log = [];
  var o = {};
  function g() {
    log.push('g');
    return o;
  }
  var p = (async function () {
    g()[await 'k'] = await 5;
  })();
  log.push('after-call');
  await p;
  return [o.k, log.join()];
}

async function evaluationOrder() {
  var log = [];
  var o = {};
  async function tick(tag, v) {
    log.push(tag);
    return v;
  }
  o[await tick('key', 'k')] = await tick('value', 9);
  log.push('done');
  return [o.k, log.join()];
}

awaitedKeyAndValue()
  .then(function (r) {
    assert.compareArray(r, [5, 5], 'o[await k] = await v');
    return awaitedKeyOnly();
  })
  .then(function (r) {
    assert.sameValue(r, 5, 'o[await k] = v');
    return awaitedValueOnly();
  })
  .then(function (r) {
    assert.sameValue(r, 5, 'o.k = await v');
    return compoundAssignment();
  })
  .then(function (r) {
    assert.sameValue(r, 6, 'o[await k] += await v');
    return nestedKeys();
  })
  .then(function (r) {
    assert.sameValue(r, 7, 'o[await a][await b] = await c');
    return awaitedBase();
  })
  .then(function (r) {
    assert.sameValue(r, 3, '(await o).k = await v');
    return baseEvaluatedBeforeAwaits();
  })
  .then(function (r) {
    assert.sameValue(r[0], 5, 'g()[await k] = await v stores through g()');
    assert.sameValue(r[1], 'g,after-call', 'the base expression is evaluated before the first await');
    return evaluationOrder();
  })
  .then(function (r) {
    assert.compareArray(r, [9, 'key,value,done'], 'key is evaluated before the value');
  })
  .then($DONE, $DONE);
