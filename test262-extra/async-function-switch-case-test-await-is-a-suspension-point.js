/*---
description: >
  An await expression in the test of a switch case clause is a suspension point
  of the async function.
esid: sec-runtime-semantics-caseblockevaluation
info: |
  Runtime Semantics: CaseBlockEvaluation

  CaseClauseIsSelected ( C, input )

  2. Let exprRef be ? Evaluation of the Expression of C.
  3. Let clauseSelector be ? GetValue(exprRef).
  4. Return IsStrictlyEqual(input, clauseSelector).

  An await expression is a valid AssignmentExpression in a case selector, so
  evaluating it suspends the async function until the operand settles, and the
  awaited value becomes the selector. Selectors are evaluated in source order,
  stopping at the first selected clause; a rejection is an abrupt completion
  delivered to the enclosing try statement.
includes: [compareArray.js]
flags: [async]
features: [async-functions]
---*/

var log = [];

async function basic() {
  switch (1) {
    case await 0:
      return 'zero';
    case await 1:
      return 'one';
  }
  return 'none';
}

async function order() {
  log = [];
  switch (2) {
    case (log.push('t1'), await 1):
      log.push('body1');
      break;
    case (log.push('t2'), await Promise.resolve(2)):
      log.push('body2');
      break;
    case (log.push('t3'), await 3):
      log.push('body3');
      break;
  }
  return log.slice();
}

async function defaultInMiddle(disc) {
  log = [];
  switch (disc) {
    case (log.push('t1'), await 1):
      log.push('body1');
    default:
      log.push('bodyD');
    case (log.push('t3'), await 3):
      log.push('body3');
      break;
  }
  return log.slice();
}

async function mixed() {
  log = [];
  switch (3) {
    case (log.push('plain1'), 1):
      return 'one';
    case (log.push('await'), await 2):
      return 'two';
    case (log.push('plain2'), 3):
      return 'three';
  }
}

async function rejectedSelector() {
  log = [];
  try {
    switch (1) {
      case await Promise.reject('E'):
        log.push('body');
        break;
      case (log.push('later'), 2):
        break;
    }
  } catch (e) {
    log.push('caught:' + e);
  }
  return log.slice();
}

async function discriminantAndSelectors() {
  switch (await 'v') {
    case await 'x':
      return 'x';
    case await 'v':
      return 'v';
  }
}

async function controlFlow() {
  var trace = [];
  outer: for (var i = 0; i < 4; i++) {
    switch (i) {
      case (await 0):
        trace.push('zero');
        break;
      case 1:
        trace.push('one');
        continue;
      case 2:
        trace.push('two');
        break outer;
    }
    trace.push('after' + i);
  }
  return trace;
}

basic()
  .then(function (v) {
    assert.sameValue(v, 'one', 'await selector selects the matching clause');
    return order();
  })
  .then(function (v) {
    assert.compareArray(v, ['t1', 't2', 'body2'], 'selectors run in source order and stop at the first match');
    return defaultInMiddle(9);
  })
  .then(function (v) {
    assert.compareArray(v, ['t1', 't3', 'bodyD', 'body3'], 'default is used after every selector fails');
    return defaultInMiddle(3);
  })
  .then(function (v) {
    assert.compareArray(v, ['t1', 't3', 'body3'], 'a clause after default can be selected');
    return mixed();
  })
  .then(function (v) {
    assert.sameValue(v, 'three', 'plain selectors after an await selector are evaluated');
    assert.compareArray(log, ['plain1', 'await', 'plain2'], 'mixed selector order');
    return rejectedSelector();
  })
  .then(function (v) {
    assert.compareArray(v, ['caught:E'], 'a rejected selector reaches the enclosing catch and skips later selectors');
    return discriminantAndSelectors();
  })
  .then(function (v) {
    assert.sameValue(v, 'v', 'await discriminant with await selectors');
    return controlFlow();
  })
  .then(function (v) {
    assert.compareArray(v, ['zero', 'after0', 'one', 'two'], 'break, continue and labeled break in a lowered switch');
  })
  .then($DONE, $DONE);
