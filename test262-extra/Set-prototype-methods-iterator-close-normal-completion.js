// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-set.prototype.issupersetof
description: >
  When isSupersetOf or isDisjointFrom stops iterating the keys of `other`
  early, it closes the iterator with a normal completion, so an error thrown
  by return() and a non-object return() result both reach the caller.
info: |
  Set.prototype.isSupersetOf ( other )
  7. Repeat, while next is not done,
    a. Set next to ? IteratorStepValue(keysIter).
    b. If next is not done, then
      i. If SetDataHas(O.[[SetData]], next) is false, then
        1. Perform ? IteratorClose(keysIter, NormalCompletion(unused)).
        2. Return false.

  Set.prototype.isDisjointFrom ( other )
  5. Else,
    ...
    c. Repeat, while next is not done,
      i. Set next to ? IteratorStepValue(keysIter).
      ii. If next is not done, then
        1. If SetDataHas(O.[[SetData]], next) is true, then
          a. Perform ? IteratorClose(keysIter, NormalCompletion(unused)).
          b. Return false.

  IteratorClose ( iteratorRecord, completion )
  ...
  6. If innerResult is a throw completion, return ? innerResult.
  7. If innerResult.[[Value]] is not an Object, throw a TypeError exception.
features: [set-methods]
---*/

function setLike(value, returnMethod) {
  return {
    size: 1,
    has: function () {
      throw new Test262Error("has must not be called");
    },
    keys: function () {
      return {
        next: function () {
          return { done: false, value: value };
        },
        return: returnMethod
      };
    }
  };
}

function throwingReturn() {
  throw new Test262Error();
}

function primitiveReturn() {
  return 5;
}

var closed = 0;
function objectReturn() {
  closed++;
  return {};
}

// isSupersetOf stops at the first value O lacks.
assert.throws(Test262Error, function () {
  new Set([1, 2, 3]).isSupersetOf(setLike(99, throwingReturn));
}, "isSupersetOf: an error thrown by return() propagates");
assert.throws(TypeError, function () {
  new Set([1, 2, 3]).isSupersetOf(setLike(99, primitiveReturn));
}, "isSupersetOf: a non-object return() result is a TypeError");
assert.sameValue(
  new Set([1, 2, 3]).isSupersetOf(setLike(99, objectReturn)),
  false,
  "isSupersetOf: an object return() result closes normally"
);
assert.sameValue(closed, 1, "isSupersetOf calls return() once");

// isDisjointFrom walks the keys of other only when O is the larger set, and
// stops at the first value O has.
assert.throws(Test262Error, function () {
  new Set([1, 2, 3]).isDisjointFrom(setLike(1, throwingReturn));
}, "isDisjointFrom: an error thrown by return() propagates");
assert.throws(TypeError, function () {
  new Set([1, 2, 3]).isDisjointFrom(setLike(1, primitiveReturn));
}, "isDisjointFrom: a non-object return() result is a TypeError");
assert.sameValue(
  new Set([1, 2, 3]).isDisjointFrom(setLike(1, objectReturn)),
  false,
  "isDisjointFrom: an object return() result closes normally"
);
assert.sameValue(closed, 2, "isDisjointFrom calls return() once");
