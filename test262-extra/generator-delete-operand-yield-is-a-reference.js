/*---
description: >
  `delete` operates on the Reference produced by its operand, so a `yield` in
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
includes: [compareArray.js]
features: [generators, optional-chaining]
---*/

function* computedKey(o) {
  var r = delete o[yield 'k'];
  return [r, 'a' in o];
}

function* yieldedBase() {
  var o = { a: 1 };
  var r = delete (yield 'base').a;
  return [r, 'a' in o];
}

function* nonReferenceOperand() {
  return delete (yield 'v');
}

function* optionalChainKey(o) {
  var r = delete o?.[yield 'k'];
  return [r, 'a' in o];
}

function* optionalChainNullishBase() {
  var o = null;
  return delete o?.[yield 'k'];
}

function* strictNonConfigurable(o) {
  'use strict';
  try {
    delete o[yield 'k'];
    return 'no-throw';
  } catch (e) {
    return e.constructor === TypeError ? 'TypeError' : 'other';
  }
}

var o1 = { a: 1 };
var it = computedKey(o1);
assert.sameValue(it.next().value, 'k', 'computedKey suspends on the key');
var res = it.next('a');
assert.sameValue(res.done, true, 'computedKey completes');
assert.compareArray(res.value, [true, false], 'delete o[yield k] deletes the property');

var o2 = { a: 1 };
it = yieldedBase();
assert.sameValue(it.next().value, 'base', 'yieldedBase suspends on the base');
res = it.next(o2);
assert.compareArray(res.value, [true, true], 'delete (yield).a deletes from the sent object, not the local');
assert.sameValue('a' in o2, false, 'the sent object lost the property');

it = nonReferenceOperand();
it.next();
assert.sameValue(it.next(5).value, true, 'delete of a non-Reference operand returns true');

var o3 = { a: 1 };
it = optionalChainKey(o3);
it.next();
assert.compareArray(it.next('a').value, [true, false], 'delete o?.[yield k] deletes the property');

it = optionalChainNullishBase();
res = it.next();
assert.sameValue(res.done, true, 'the short-circuited chain never reaches the yield');
assert.sameValue(res.value, true, 'delete on a short-circuited optional chain returns true');

var frozen = {};
Object.defineProperty(frozen, 'k', { value: 1, configurable: false });
it = strictNonConfigurable(frozen);
it.next();
assert.sameValue(it.next('k').value, 'TypeError', 'strict delete of a non-configurable property throws');
