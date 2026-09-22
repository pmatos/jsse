// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgeneratorawaitreturn
description: >
  When PromiseResolve(%Promise%, value) throws, AsyncGeneratorAwaitReturn
  rejects the request in the same job and drains the queue, so a request made
  after it is served next.
info: |
  AsyncGeneratorAwaitReturn ( generator )

  [...]
  7. Let promiseCompletion be Completion(PromiseResolve(%Promise%, completion.[[Value]])).
  8. If promiseCompletion is an abrupt completion, then
     a. Perform AsyncGeneratorCompleteStep(generator, promiseCompletion, true).
     b. Perform AsyncGeneratorDrainQueue(generator).
     c. Return unused.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration]
---*/

function throwingOperand() {
  var p = Promise.resolve(1);
  Object.defineProperty(p, 'constructor', { get: function () { throw 'C'; } });
  return p;
}

function flush() {
  var p = Promise.resolve();
  for (var i = 0; i < 6; i++) p = p.then(function () {});
  return p;
}

async function* g() { yield 1; }

asyncTest(async function () {
  var log = [];
  var it = g();
  var first = it.return(throwingOperand()).then(
    function () { log.push('return-fulfilled'); },
    function (e) { log.push('return-rejected:' + e); }
  );
  var second = it.next().then(function (r) { log.push('next:' + r.done); });
  await Promise.all([first, second, flush()]);
  assert.compareArray(log, ['return-rejected:C', 'next:true'], 'suspended-start');

  log = [];
  it = g();
  await it.next();
  await it.return();
  first = it.return(throwingOperand()).then(
    function () { log.push('return-fulfilled'); },
    function (e) { log.push('return-rejected:' + e); }
  );
  second = it.next().then(function (r) { log.push('next:' + r.done); });
  await Promise.all([first, second, flush()]);
  assert.compareArray(log, ['return-rejected:C', 'next:true'], 'completed');
});
