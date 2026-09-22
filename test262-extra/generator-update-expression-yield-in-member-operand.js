/*---
description: >
  `++` and `--` read and write through the same Reference, so a `yield` in the
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
includes: [compareArray.js]
features: [generators, BigInt]
---*/

function* postfixIncrement(o) {
  var r = o[yield 'k']++;
  return [r, o.k];
}

function* prefixIncrement(o) {
  var r = ++o[yield 'k'];
  return [r, o.k];
}

function* postfixDecrement(o) {
  var r = o[yield 'k']--;
  return [r, o.k];
}

function* prefixDecrement(o) {
  var r = --o[yield 'k'];
  return [r, o.k];
}

function* yieldedBase() {
  (yield 'base').k++;
}

function* nestedComputedKey(o) {
  o[yield 'a'].k++;
  return o.a.k;
}

function* stringOperand(o) {
  var r = o[yield 'k']++;
  return [r, o.k];
}

function* bigintOperand(o) {
  var r = o[yield 'k']++;
  return [r, o.k];
}

function run(gen, sent) {
  var it = gen;
  var res = it.next();
  while (!res.done) res = it.next(sent);
  return res.value;
}

assert.compareArray(run(postfixIncrement({ k: 1 }), 'k'), [1, 2], 'o[yield k]++');
assert.compareArray(run(prefixIncrement({ k: 1 }), 'k'), [2, 2], '++o[yield k]');
assert.compareArray(run(postfixDecrement({ k: 1 }), 'k'), [1, 0], 'o[yield k]--');
assert.compareArray(run(prefixDecrement({ k: 1 }), 'k'), [0, 0], '--o[yield k]');

var target = { k: 1 };
var it = yieldedBase();
it.next();
it.next(target);
assert.sameValue(target.k, 2, '(yield).k++ updates the sent object');

assert.sameValue(run(nestedComputedKey({ a: { k: 1 } }), 'a'), 2, 'o[yield a].k++');
assert.compareArray(run(stringOperand({ k: '5' }), 'k'), [5, 6], 'ToNumeric on the old value');

var big = run(bigintOperand({ k: 1n }), 'k');
assert.sameValue(big[0], 1n, 'BigInt old value');
assert.sameValue(big[1], 2n, 'BigInt new value');
