// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgenerator-prototype-next
description: >
  next, return and throw on an invalid receiver return a rejected promise to
  the caller without running any queued job first.
info: |
  %AsyncGeneratorPrototype%.next ( value )

  1. Let generator be the this value.
  2. Let promiseCapability be ! NewPromiseCapability(%Promise%).
  3. Let result be Completion(AsyncGeneratorValidate(generator, empty)).
  4. IfAbruptRejectPromise(result, promiseCapability).

  IfAbruptRejectPromise ( value, capability )

  1. Assert: value is a Completion Record.
  2. If value is an abrupt completion, then
     a. Perform ? Call(capability.[[Reject]], undefined, « value.[[Value]] »).
     b. Return capability.[[Promise]].

  The same steps apply to %AsyncGeneratorPrototype%.return and
  %AsyncGeneratorPrototype%.throw. Settling a promise only enqueues its
  reaction jobs, and a Job runs to completion only when the execution context
  stack is empty, so a job queued before the call must not run before the call
  returns.

  A witness chain of promise reactions is started before the call, so the
  position of "sync-end" within the witness log shows whether queued jobs ran
  inside the call.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration]
---*/

var AsyncGeneratorPrototype = Object.getPrototypeOf(async function* () {}).prototype;

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

var expected = ['sync-end', 'w1', 'rejected', 'w2', 'w3'];

asyncTest(async function () {
  var log = await observe(function () {
    return AsyncGeneratorPrototype.next.call({});
  });
  assert.compareArray(log, expected, 'next on an ordinary object');

  log = await observe(function () {
    return AsyncGeneratorPrototype.return.call(1, 'v');
  });
  assert.compareArray(log, expected, 'return on a primitive');

  log = await observe(function () {
    return AsyncGeneratorPrototype.throw.call(null, 'e');
  });
  assert.compareArray(log, expected, 'throw on null');
});
