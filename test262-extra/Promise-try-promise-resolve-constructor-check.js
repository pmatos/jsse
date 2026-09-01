/*---
description: >
  Promise.try uses PromiseResolve for a callback's normal completion.
esid: sec-promise.try
info: |
  PromiseResolve ( C, x ) reads a promise's "constructor" property once and
  returns x unchanged when that value is C. When the constructors differ, it
  creates and resolves a new promise using C.

  Regression test for issue #553.
features: [promise-try, class]
---*/

var constructorReads = 0;
var sentinel = Promise.resolve(1);
Object.defineProperty(sentinel, "constructor", {
  get: function () {
    constructorReads += 1;
    return Promise;
  },
});

var sameConstructorResult = Promise.try(function () {
  return sentinel;
});

assert.sameValue(constructorReads, 1, "constructor is read exactly once");
assert.sameValue(
  sameConstructorResult,
  sentinel,
  "a promise from the receiver is returned unchanged"
);

class SubPromise extends Promise {}

var basePromise = Promise.resolve(2);
var differentConstructorResult = SubPromise.try(function () {
  return basePromise;
});

assert.notSameValue(
  differentConstructorResult,
  basePromise,
  "a promise from a different constructor is wrapped"
);
assert.sameValue(
  differentConstructorResult instanceof SubPromise,
  true,
  "the wrapper is created by the receiver"
);
