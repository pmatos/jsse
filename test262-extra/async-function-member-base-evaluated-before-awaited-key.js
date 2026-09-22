/*---
description: >
  The base of a property reference is evaluated before an `await` in its
  computed key, both when the reference is read and when it is the callee of
  a call.
esid: sec-property-accessors-runtime-semantics-evaluation
info: |
  MemberExpression : MemberExpression [ Expression ]

  1. Let baseReference be ? Evaluation of MemberExpression.
  2. Let baseValue be ? GetValue(baseReference).
  3. Return ? EvaluatePropertyAccessWithExpressionKey(baseValue, Expression, strict).

  EvaluatePropertyAccessWithExpressionKey evaluates the key expression only
  after the base value has been obtained.
flags: [async]
includes: [compareArray.js]
features: [async-functions]
---*/

function later(v, sideEffect) {
  return Promise.resolve().then(function () {
    if (sideEffect) sideEffect();
    return v;
  });
}

async function readBaseBeforeKey() {
  var log = [];
  var o = { k: 7 };
  function base() {
    log.push('base');
    return o;
  }
  var p = (async function () {
    return base()[await later('k', function () {
      log.push('key');
    })];
  })();
  log.push('after-call');
  var v = await p;
  return [v, log.join()];
}

async function callBaseBeforeKey() {
  var log = [];
  var o = {
    m: function () {
      return this === o;
    },
  };
  function base() {
    log.push('base');
    return o;
  }
  var p = (async function () {
    return base()[await later('m', function () {
      log.push('key');
    })]();
  })();
  log.push('after-call');
  var v = await p;
  return [v, log.join()];
}

async function baseValueCapturedBeforeKey() {
  var o1 = { k: 'first' };
  var o2 = { k: 'second' };
  var o = o1;
  return o[await later('k', function () {
    o = o2;
  })];
}

readBaseBeforeKey()
  .then(function (r) {
    assert.sameValue(r[0], 7, 'value read through the base');
    assert.sameValue(r[1], 'base,after-call,key', 'base is evaluated before the awaited key');
    return callBaseBeforeKey();
  })
  .then(function (r) {
    assert.sameValue(r[0], true, 'the base is the this value of the call');
    assert.sameValue(r[1], 'base,after-call,key', 'callee base is evaluated before the awaited key');
    return baseValueCapturedBeforeKey();
  })
  .then(function (r) {
    assert.sameValue(r, 'first', 'the base value is captured before the key suspends');
  })
  .then($DONE, $DONE);
