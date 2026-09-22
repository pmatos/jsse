/*---
description: >
  `delete` operates on the Reference produced by its operand, so an `await` in
  the operand's base or computed key must not turn the operand into a value
  before the property is deleted.
esid: sec-delete-operator-runtime-semantics-evaluation
info: |
  UnaryExpression : delete UnaryExpression

  1. Let ref be ? Evaluation of UnaryExpression.
  2. If ref is not a Reference Record, return true.
  ...
  5. Else,
    a. Assert: ref is a Property Reference.
    ...
    d. Let baseObj be ? ToObject(ref.[[Base]]).
    e. Let deleteStatus be ? baseObj.[[Delete]](refName).
    f. If deleteStatus is false and ref.[[Strict]] is true, throw a TypeError exception.
    g. Return deleteStatus.
flags: [async]
includes: [compareArray.js]
features: [async-functions, optional-chaining]
---*/

async function computedKey() {
  var o = { a: 1 };
  var r = delete o[await 'a'];
  return [r, 'a' in o];
}

async function awaitedBase() {
  var o = { a: 1 };
  var r = delete (await o).a;
  return [r, 'a' in o];
}

async function awaitedBaseComputedKey() {
  var o = { a: 1 };
  var r = delete (await o)[await 'a'];
  return [r, 'a' in o];
}

async function nonReferenceOperand() {
  return delete (await 5);
}

async function optionalChainKey() {
  var o = { a: 1 };
  var r = delete o?.[await 'a'];
  return [r, 'a' in o];
}

async function optionalChainNullishBase() {
  var o = null;
  var r = delete o?.[await 'a'];
  return r;
}

async function nestedMissingProperty() {
  var o = { a: 1 };
  var r = delete o.a[await 'x'];
  return [r, o.a];
}

async function baseEvaluatedBeforeAwait() {
  var log = [];
  var o = { a: 1 };
  function g() {
    log.push('g');
    return o;
  }
  var p = (async function () {
    return delete g()[await 'a'];
  })();
  log.push('after-call');
  var r = await p;
  return [r, 'a' in o, log.join()];
}

async function strictNonConfigurable() {
  'use strict';
  var o = {};
  Object.defineProperty(o, 'k', { value: 1, configurable: false });
  var log = [];
  try {
    delete o[await 'k'];
    log.push('no-throw');
  } catch (e) {
    log.push(e.constructor === TypeError ? 'TypeError' : 'other');
  }
  return log.join();
}

computedKey()
  .then(function (r) {
    assert.compareArray(r, [true, false], 'delete o[await k] deletes the property');
    return awaitedBase();
  })
  .then(function (r) {
    assert.compareArray(r, [true, false], 'delete (await o).a deletes the property');
    return awaitedBaseComputedKey();
  })
  .then(function (r) {
    assert.compareArray(r, [true, false], 'delete (await o)[await k] deletes the property');
    return nonReferenceOperand();
  })
  .then(function (r) {
    assert.sameValue(r, true, 'delete of a non-Reference operand returns true');
    return optionalChainKey();
  })
  .then(function (r) {
    assert.compareArray(r, [true, false], 'delete o?.[await k] deletes the property');
    return optionalChainNullishBase();
  })
  .then(function (r) {
    assert.sameValue(r, true, 'delete on a short-circuited optional chain returns true');
    return nestedMissingProperty();
  })
  .then(function (r) {
    assert.compareArray(r, [true, 1], 'delete o.a[await k] on a primitive base returns true');
    return baseEvaluatedBeforeAwait();
  })
  .then(function (r) {
    assert.sameValue(r[0], true, 'delete g()[await k] result');
    assert.sameValue(r[1], false, 'delete g()[await k] deletes the property');
    assert.sameValue(r[2], 'g,after-call', 'the base expression is evaluated before the await');
    return strictNonConfigurable();
  })
  .then(function (r) {
    assert.sameValue(r, 'TypeError', 'strict delete of a non-configurable property throws after the await');
  })
  .then($DONE, $DONE);
