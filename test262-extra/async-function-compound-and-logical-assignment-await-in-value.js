/*---
description: >
  An assignment whose right-hand side contains `await` still evaluates the
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
flags: [async]
includes: [compareArray.js]
features: [async-functions, logical-assignment-operators, coalesce-expression]
---*/

function later(v, sideEffect) {
  return Promise.resolve().then(function () {
    if (sideEffect) sideEffect();
    return v;
  });
}

async function compoundReadsOldValueFirst() {
  var x = 1;
  x += await later(5, function () {
    x = 10;
  });
  var o = { k: 1 };
  o.k += await later(5, function () {
    o.k = 10;
  });
  var s = 'a';
  s += await later('b', function () {
    s = 'Z';
  });
  var p = { k: 1 };
  p['k'] -= await later(5, function () {
    p.k = 10;
  });
  return [x, o.k, s, p.k];
}

async function logicalAssignmentShortCircuits() {
  var calls = 0;
  function rhs() {
    calls++;
    return later(2);
  }
  var a = 1;
  a ||= await rhs();
  var b = 0;
  b &&= await rhs();
  var c = 0;
  c ??= await rhs();
  var d = 1;
  d ??= await rhs();
  var o = { p: 1, q: 0, r: 0 };
  o.p ||= await rhs();
  o.q &&= await rhs();
  o.r ??= await rhs();
  o['p'] ||= await rhs();
  o[await 'p'] ||= await rhs();
  o[await 'q'] &&= await rhs();
  return [a, b, c, d, o.p, o.q, o.r, calls];
}

async function logicalAssignmentEvaluatesRight() {
  var calls = 0;
  function rhs(v) {
    calls++;
    return later(v);
  }
  var a = 0;
  a ||= await rhs(2);
  var b = 1;
  b &&= await rhs(3);
  var c = null;
  c ??= await rhs(4);
  var o = { p: 0, q: 1, r: undefined };
  o.p ||= await rhs(5);
  o.q &&= await rhs(6);
  o.r ??= await rhs(7);
  var result = (o[await 's'] ||= await rhs(8));
  return [a, b, c, o.p, o.q, o.r, o.s, result, calls];
}

async function targetEvaluatedBeforeRightHandSide() {
  var log = [];
  var o = {};
  function base() {
    log.push('base');
    return o;
  }
  function key() {
    log.push('key');
    return 'k';
  }
  var p = (async function () {
    base()[key()] = await later(1, function () {
      log.push('value');
    });
  })();
  log.push('after-call');
  await p;
  return [o.k, log.join()];
}

async function baseCapturedBeforeAwait() {
  var o1 = {};
  var o2 = {};
  var o = o1;
  o.k = await later(1, function () {
    o = o2;
  });
  var key = 'a';
  var t = {};
  t[key] = await later(2, function () {
    key = 'b';
  });
  return [o1.k, o2.k, t.a, t.b];
}

async function baseSuspendsKeyDoesNot() {
  var log = [];
  var o = { k: 1 };
  function key() {
    log.push('key');
    return 'k';
  }
  var p = (async function () {
    (await o)[key()] += await later(2, function () {
      log.push('value');
    });
  })();
  log.push('after-call');
  await p;
  return [o.k, log.join()];
}

compoundReadsOldValueFirst()
  .then(function (r) {
    assert.compareArray(r, [6, 6, 'ab', -4], 'compound assignment reads the target before the awaited value');
    return logicalAssignmentShortCircuits();
  })
  .then(function (r) {
    assert.compareArray(r, [1, 0, 0, 1, 1, 0, 0, 0], 'short-circuiting logical assignment never evaluates the right-hand side');
    return logicalAssignmentEvaluatesRight();
  })
  .then(function (r) {
    assert.compareArray(r, [2, 3, 4, 5, 6, 7, 8, 8, 7], 'non-short-circuiting logical assignment evaluates the right-hand side');
    return targetEvaluatedBeforeRightHandSide();
  })
  .then(function (r) {
    assert.sameValue(r[0], 1, 'value is stored through the target');
    assert.sameValue(r[1], 'base,key,after-call,value', 'base and key are evaluated before the first await');
    return baseCapturedBeforeAwait();
  })
  .then(function (r) {
    assert.compareArray(r, [1, undefined, 2, undefined], 'base and key are captured before the right-hand side runs');
    return baseSuspendsKeyDoesNot();
  })
  .then(function (r) {
    assert.sameValue(r[0], 3, 'compound assignment through an awaited base');
    assert.sameValue(r[1], 'after-call,key,value', 'the key runs after the awaited base and before the value');
  })
  .then($DONE, $DONE);
