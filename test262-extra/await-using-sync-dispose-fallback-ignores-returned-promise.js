// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-getdisposemethod
description: >
  When an async-dispose resource only has a synchronous @@dispose method, the
  promise it may return is discarded rather than awaited.
info: |
  GetDisposeMethod ( V, hint )

  1. If hint is async-dispose, then
     a. Let method be ? GetMethod(V, %Symbol.asyncDispose%).
     b. If method is undefined, then
        i. Set method to ? GetMethod(V, %Symbol.dispose%).
        ii. If method is not undefined, then
            1. Let closure be a new Abstract Closure with no parameters that captures method and performs the following steps when called:
               a. Let O be the this value.
               b. Let promiseCapability be ! NewPromiseCapability(%Promise%).
               c. Let result be Completion(Call(method, O)).
               d. IfAbruptRejectPromise(result, promiseCapability).
               e. Perform ! Call(promiseCapability.[[Resolve]], undefined, « undefined »).
               f. Return promiseCapability.[[Promise]].
            2. NOTE: This function is not observable to user code. It is used to ensure that a Promise returned from a synchronous @@dispose method will not be awaited.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management]
---*/

asyncTest(async function () {
  var neverResolves = new Promise(function () {});
  var log = [];

  async function viaAwaitUsing() {
    await using x = {
      [Symbol.dispose]() { log.push('await using'); return neverResolves; }
    };
  }
  await viaAwaitUsing();

  var stack = new AsyncDisposableStack();
  stack.use({
    [Symbol.dispose]() { log.push('disposeAsync'); return neverResolves; }
  });
  await stack.disposeAsync();

  assert.compareArray(log, ['await using', 'disposeAsync']);

  var thrown = new Test262Error();
  async function throwing() {
    await using x = {
      [Symbol.dispose]() { throw thrown; }
    };
  }
  await assert.throwsAsync(Test262Error, throwing);
});
