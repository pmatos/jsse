// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-async-function-definitions-EvaluateAsyncFunctionBody
description: >
  An async function whose FunctionDeclarationInstantiation throws returns its
  rejected promise to the caller without running any queued job first.
info: |
  EvaluateAsyncFunctionBody ( AsyncFunctionBody )

  1. Let promiseCapability be ! NewPromiseCapability(%Promise%).
  2. Let declResult be Completion(FunctionDeclarationInstantiation(functionObject, argumentsList)).
  3. If declResult is an abrupt completion, then
     a. Perform ! Call(promiseCapability.[[Reject]], undefined, « declResult.[[Value]] »).
  4. Else,
     a. Perform AsyncFunctionStart(promiseCapability, FunctionBody).
  5. Return Completion Record { [[Type]]: return, [[Value]]: promiseCapability.[[Promise]], [[Target]]: empty }.

  Settling a promise only enqueues its reaction jobs, and a Job runs to
  completion only when the execution context stack is empty, so a job queued
  before the call must not run before the call returns.

  A witness chain of promise reactions is started before the call, so the
  position of "sync-end" within the witness log shows whether queued jobs ran
  inside the call.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
---*/

function observe(shape) {
  var log = [];
  var L = function (entry) { log.push(entry); };
  Promise.resolve()
    .then(function () { L('w1'); })
    .then(function () { L('w2'); })
    .then(function () { L('w3'); });
  var promise = shape(L);
  promise.then(function () { L('resolved'); }, function () { L('rejected'); });
  L('sync-end');
  var drain = Promise.resolve();
  for (var i = 0; i < 12; i++) {
    drain = drain.then(function () {});
  }
  return drain.then(function () { return log; });
}

function thrower() { throw 1; }

var expected = ['sync-end', 'w1', 'rejected', 'w2', 'w3'];

asyncTest(async function () {
  var log = await observe(function () {
    async function f(a = thrower()) {}
    return f();
  });
  assert.compareArray(log, expected, 'async function declaration');

  log = await observe(function () {
    var f = async (a = thrower()) => {};
    return f();
  });
  assert.compareArray(log, expected, 'async arrow function');

  log = await observe(function () {
    var o = { async m(a = thrower()) {} };
    return o.m();
  });
  assert.compareArray(log, expected, 'async method');

  log = await observe(function () {
    async function f({ a } = null) {}
    return f();
  });
  assert.compareArray(log, expected, 'throwing destructuring parameter');
});
