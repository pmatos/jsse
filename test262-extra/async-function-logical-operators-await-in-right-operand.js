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
flags: [async]
includes: [compareArray.js]
features: [async-functions, coalesce-expression, IsHTMLDDA]
---*/

async function nullishFallsThrough() {
  var a = null ?? await 3;
  var b = undefined ?? await 4;
  var c = (await null) ?? await 5;
  var d = (await undefined) ?? await 6;
  return [a, b, c, d];
}

async function nullishShortCircuits() {
  var a = 1 ?? await 3;
  var b = 0 ?? await 3;
  var c = '' ?? await 3;
  var d = (await 1) ?? await 3;
  var e = (await 0) ?? await 3;
  var f = ((await null) ?? 5) ?? await 3;
  return [a, b, c, d, e, f];
}

async function orOperator() {
  return [
    1 || await 3,
    0 || await 3,
    (await 1) || await 3,
    (await 0) || await 3,
  ];
}

async function andOperator() {
  return [
    0 && await 3,
    1 && await 3,
    (await 0) && await 3,
    (await 1) && await 3,
  ];
}

async function leftEvaluatedOnce() {
  var calls = 0;
  function f() {
    calls++;
    return 0;
  }
  var a = f() && await 3;
  var b = f() || await 4;
  return [a, b, calls];
}

async function boundThroughDeclarations() {
  var v = null ?? await 'var';
  let l = 1 && await 'let';
  const c = 0 || await 'const';
  function id(x, y) {
    return [x, y];
  }
  var args = id(0 && await 1, null ?? await 2);
  return [v, l, c, args];
}

async function htmldda() {
  var dda = $262.IsHTMLDDA;
  var a = dda ?? await 3;
  var b = (await dda) ?? await 3;
  return [a === dda, b === dda];
}

async function shortCircuitDoesNotSuspend() {
  var log = [];
  var p = (async function () {
    var r = 1 ?? await 0;
    var s = 0 && await 0;
    var t = 1 || await 0;
    log.push([r, s, t].join());
  })();
  log.push('sync');
  await p;
  return log;
}

async function fallThroughSuspends() {
  var log = [];
  var p = (async function () {
    var r = null ?? await 7;
    log.push('r' + r);
  })();
  log.push('sync');
  await p;
  return log;
}

nullishFallsThrough()
  .then(function (r) {
    assert.compareArray(r, [3, 4, 5, 6], 'nullish left evaluates the awaited right operand');
    return nullishShortCircuits();
  })
  .then(function (r) {
    assert.compareArray(r, [1, 0, '', 1, 0, 5], 'non-nullish left is the result');
    return orOperator();
  })
  .then(function (r) {
    assert.compareArray(r, [1, 3, 1, 3], '||');
    return andOperator();
  })
  .then(function (r) {
    assert.compareArray(r, [0, 3, 0, 3], '&&');
    return leftEvaluatedOnce();
  })
  .then(function (r) {
    assert.compareArray(r, [0, 4, 2], 'the left operand is evaluated exactly once');
    return boundThroughDeclarations();
  })
  .then(function (r) {
    assert.sameValue(r[0], 'var', 'var binding');
    assert.sameValue(r[1], 'let', 'let binding');
    assert.sameValue(r[2], 'const', 'const binding');
    assert.compareArray(r[3], [0, 2], 'call arguments');
    return htmldda();
  })
  .then(function (r) {
    assert.compareArray(r, [true, true], 'an IsHTMLDDA object is not nullish');
    return shortCircuitDoesNotSuspend();
  })
  .then(function (r) {
    assert.compareArray(r, ['1,0,1', 'sync'], 'a short-circuited right operand is never awaited');
    return fallThroughSuspends();
  })
  .then(function (r) {
    assert.compareArray(r, ['sync', 'r7'], 'an evaluated right operand suspends');
  })
  .then($DONE, $DONE);
