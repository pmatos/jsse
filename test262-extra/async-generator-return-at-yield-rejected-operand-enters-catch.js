// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgeneratorunwrapyieldresumption
description: >
  A `.return(v)` whose operand `v` rejects is not a return completion: the
  rejection is thrown into the generator at the yield, so a surrounding
  `catch` can handle it, a `finally` still runs, and with no handler the
  request rejects with the reason. Parked in `yield*` it is a throw
  completion, so the delegate's `throw` is called rather than its `return`.
info: |
  AsyncGeneratorUnwrapYieldResumption ( resumptionValue )

  1. If resumptionValue is not a return completion, return ? resumptionValue.
  2. Let awaited be Completion(Await(resumptionValue.[[Value]])).
  3. If awaited is a throw completion, return ? awaited.

  YieldExpression : yield * AssignmentExpression

  7.b. Else if received is a throw completion, then
       i. Let throw be ? GetMethod(iterator, "throw").
       ii. If throw is not undefined, then
           1. Let innerResult be ? Call(throw, iterator, « received.[[Value]] »).
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration]
---*/

asyncTest(async function () {
  var log = [];
  var L = function (entry) { log.push(entry); };

  var it = (async function* () {
    try { yield 1; } catch (e) { L('caught:' + e); yield 'c'; }
  })();
  await it.next();
  var r = await it.return(Promise.reject('E'));
  assert.compareArray(log, ['caught:E'], 'the rejection is thrown into the generator at the yield');
  assert.sameValue(r.value, 'c', 'the catch block yields');
  assert.sameValue(r.done, false, 'the generator is still running');
  r = await it.next();
  assert.sameValue(r.done, true, 'the generator completes after the catch');

  log = [];
  it = (async function* () {
    try { yield 1; } finally { L('fin'); }
  })();
  await it.next();
  var rejected;
  try {
    await it.return(Promise.reject('E2'));
    rejected = 'not rejected';
  } catch (e) {
    rejected = e;
  }
  assert.sameValue(rejected, 'E2', 'with no catch the request rejects with the reason');
  assert.compareArray(log, ['fin'], 'the finally still runs');
  r = await it.next();
  assert.sameValue(r.done, true, 'the generator is completed');

  it = (async function* () { yield 1; })();
  await it.next();
  try {
    await it.return(Promise.reject('E3'));
    rejected = 'not rejected';
  } catch (e) {
    rejected = e;
  }
  assert.sameValue(rejected, 'E3', 'with no try the request rejects with the reason');
  r = await it.next();
  assert.sameValue(r.done, true, 'and the generator is completed');

  log = [];
  var inner = {
    [Symbol.asyncIterator]() { return this; },
    next() { return Promise.resolve({ value: 'i1', done: false }); },
    return(v) { L('inner-return:' + v); return { value: v, done: true }; },
    throw(e) { L('inner-throw:' + e); return { value: 'thrown', done: true }; }
  };
  it = (async function* () {
    var result = yield* inner;
    L('delegate-result:' + result);
    return 'end';
  })();
  await it.next();
  r = await it.return(Promise.reject('E4'));
  assert.compareArray(
    log,
    ['inner-throw:E4', 'delegate-result:thrown'],
    'the delegate receives a throw, not a return'
  );
  assert.sameValue(r.value, 'end', 'the generator resumes after the yield* and finishes');
  assert.sameValue(r.done, true, 'and is done');

  log = [];
  it = (async function* () {
    var result = yield* inner;
    L('delegate-result:' + result);
    return 'end';
  })();
  var first = it.next();
  var queuedReturn = it.return(Promise.reject('E5'));
  r = await first;
  assert.sameValue(r.value, 'i1', 'the first next yields the delegate value');
  r = await queuedReturn;
  assert.compareArray(
    log,
    ['inner-throw:E5', 'delegate-result:thrown'],
    'a return queued behind the yield* step also delivers the rejection as a throw'
  );
  assert.sameValue(r.value, 'end', 'the generator resumes after the yield* and finishes');
  assert.sameValue(r.done, true, 'and is done');
});
