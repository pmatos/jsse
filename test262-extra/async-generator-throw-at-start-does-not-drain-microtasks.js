// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgenerator-prototype-throw
description: >
  throw on a suspended-start or completed async generator rejects its promise
  and returns it to the caller without running any queued job first.
info: |
  %AsyncGeneratorPrototype%.throw ( exception )

  [...]
  5. If state is suspended-start, then
     a. Set generator.[[AsyncGeneratorState]] to completed.
     b. Set state to completed.
  6. If state is completed, then
     a. Perform ! Call(promiseCapability.[[Reject]], undefined, « exception »).
     b. Return promiseCapability.[[Promise]].

  Settling a promise only enqueues its reaction jobs, and a Job runs to
  completion only when the execution context stack is empty, so a job queued
  before the call must not run before the call returns.

  A witness chain of promise reactions is started before the call, so the
  position of "sync-end" within the witness log shows whether queued jobs ran
  inside the call.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration]
---*/

async function* g() {}

function observe(shape) {
  var log = [];
  var L = function (entry) { log.push(entry); };
  Promise.resolve()
    .then(function () { L('w1'); })
    .then(function () { L('w2'); })
    .then(function () { L('w3'); })
    .then(function () { L('w4'); });
  var promise = shape();
  promise.then(function () { L('resolved'); }, function () { L('rejected'); });
  L('sync-end');
  var drain = Promise.resolve();
  for (var i = 0; i < 12; i++) {
    drain = drain.then(function () {});
  }
  return drain.then(function () { return log; });
}

async function completed() {
  var it = g();
  await it.next();
  return it;
}

var expected = ['sync-end', 'w1', 'rejected', 'w2', 'w3', 'w4'];

asyncTest(async function () {
  var it = g();
  var log = await observe(function () { return it.throw('E'); });
  assert.compareArray(log, expected, 'throw at suspended-start');

  it = await completed();
  log = await observe(function () { return it.throw('E'); });
  assert.compareArray(log, expected, 'throw on a completed generator');
});
