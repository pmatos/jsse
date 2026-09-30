/*---
description: >
  `++` and `--` read and write through the same Reference, so an `await` in the
  operand's base or computed key must not turn the operand into a value copy.
esid: sec-postfix-increment-operator-runtime-semantics-evaluation
info: |
  UpdateExpression : LeftHandSideExpression ++

  1. Let lhs be ? Evaluation of LeftHandSideExpression.
  2. Let oldValue be ? ToNumeric(? GetValue(lhs)).
  3. If oldValue is a Number, let newValue be Number::add(oldValue, 1).
  4. Else, let newValue be BigInt::add(oldValue, 1n).
  5. Perform ? PutValue(lhs, newValue).
  6. Return oldValue.

  The prefix forms and the `--` forms (sec-prefix-increment-operator-runtime-semantics-evaluation,
  sec-postfix-decrement-operator-runtime-semantics-evaluation,
  sec-prefix-decrement-operator-runtime-semantics-evaluation) follow the same
  read-then-PutValue-through-the-same-Reference pattern.
flags: [async]
includes: [compareArray.js]
features: [async-functions, BigInt]
---*/

async function postfixIncrement() {
  var o = { k: 1 };
  var r = o[await 'k']++;
  return [r, o.k];
}

async function prefixIncrement() {
  var o = { k: 1 };
  var r = ++o[await 'k'];
  return [r, o.k];
}

async function postfixDecrement() {
  var o = { k: 1 };
  var r = o[await 'k']--;
  return [r, o.k];
}

async function prefixDecrement() {
  var o = { k: 1 };
  var r = --o[await 'k'];
  return [r, o.k];
}

async function nestedComputedKey() {
  var o = { a: { k: 1 } };
  o[await 'a'].k++;
  return o.a.k;
}

async function awaitedBase() {
  var o = { k: 1 };
  (await o).k++;
  return o.k;
}

async function awaitedBaseAndKey() {
  var o = { k: 1 };
  var r = (await o)[await 'k']++;
  return [r, o.k];
}

async function bigintOperand() {
  var o = { k: 1n };
  var r = o[await 'k']++;
  return [r, o.k];
}

async function stringOperandIsCoercedToNumber() {
  var o = { k: '5' };
  var r = o[await 'k']++;
  return [r, o.k];
}

async function baseEvaluatedBeforeAwait() {
  var log = [];
  var o = { k: 1 };
  function g() {
    log.push('g');
    return o;
  }
  var p = (async function () {
    g()[await 'k']++;
  })();
  log.push('after-call');
  await p;
  return [o.k, log.join()];
}

postfixIncrement()
  .then(function (r) {
    assert.compareArray(r, [1, 2], 'o[await k]++ returns the old value and stores the new one');
    return prefixIncrement();
  })
  .then(function (r) {
    assert.compareArray(r, [2, 2], '++o[await k] returns and stores the new value');
    return postfixDecrement();
  })
  .then(function (r) {
    assert.compareArray(r, [1, 0], 'o[await k]-- returns the old value and stores the new one');
    return prefixDecrement();
  })
  .then(function (r) {
    assert.compareArray(r, [0, 0], '--o[await k] returns and stores the new value');
    return nestedComputedKey();
  })
  .then(function (r) {
    assert.sameValue(r, 2, 'o[await a].k++ updates the nested property');
    return awaitedBase();
  })
  .then(function (r) {
    assert.sameValue(r, 2, '(await o).k++ updates the property');
    return awaitedBaseAndKey();
  })
  .then(function (r) {
    assert.compareArray(r, [1, 2], '(await o)[await k]++ updates the property');
    return bigintOperand();
  })
  .then(function (r) {
    assert.sameValue(r[0], 1n, 'BigInt postfix result is the old value');
    assert.sameValue(r[1], 2n, 'BigInt operand is incremented');
    return stringOperandIsCoercedToNumber();
  })
  .then(function (r) {
    assert.compareArray(r, [5, 6], 'the old value is ToNumeric-coerced, the new value stored');
    return baseEvaluatedBeforeAwait();
  })
  .then(function (r) {
    assert.sameValue(r[0], 2, 'g()[await k]++ updates the property');
    assert.sameValue(r[1], 'g,after-call', 'the base expression is evaluated before the await');
  })
  .then($DONE, $DONE);
