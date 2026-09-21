// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgeneratorstart
description: >
  A function-level `await using` in an async generator suspends the generator
  at each Await of DisposeResources, so the request promise settles only after
  the async disposer's own promise does, instead of draining the job queue
  inline and settling early.
info: |
  AsyncGeneratorStart ( generator, generatorBody )

  4. Let result be Completion(Evaluation of generatorBody).
  [...]
  i. If result is a normal completion, set result to NormalCompletion(undefined).
  [...]
  k. Perform AsyncGeneratorCompleteStep(acGenerator, result, true).
  l. Perform AsyncGeneratorDrainQueue(acGenerator).

  DisposeResources ( disposeCapability, completion ) is part of evaluating the
  body (proposal-explicit-resource-management, sec-disposeresources), so the
  request is completed only after every Await it performs.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management, async-iteration]
---*/

function observe(shape) {
  var log = [];
  var L = function (entry) { log.push(entry); };
  var release;
  var gate = new Promise(function (resolve) { release = resolve; });
  var it = shape(L, gate);
  it.next().then(
    function (r) { L('n1:' + r.value + ':' + r.done); },
    function (e) { L('n1-rejected:' + e); }
  );
  L('sync-end');
  var drain = Promise.resolve();
  for (var i = 0; i < 12; i++) {
    drain = drain.then(function () {});
  }
  return drain.then(function () {
    L('release');
    release();
    var after = Promise.resolve();
    for (var j = 0; j < 12; j++) {
      after = after.then(function () {});
    }
    return after;
  }).then(function () { return log; });
}

asyncTest(async function () {
  var log = await observe(function (L, gate) {
    return (async function* () {
      await using a = {
        async [Symbol.asyncDispose]() {
          L('d-start');
          await gate;
          L('d-end');
        }
      };
      L('body');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'd-start', 'sync-end', 'release', 'd-end', 'n1:undefined:true'],
    'the request settles after a gate-controlled async disposer finishes'
  );

  log = await observe(function (L, gate) {
    return (async function* () {
      await using a = {
        async [Symbol.asyncDispose]() {
          L('d-start');
          await gate;
          L('d-end');
          throw 'disposer-error';
        }
      };
      L('body');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'd-start', 'sync-end', 'release', 'd-end', 'n1-rejected:disposer-error'],
    'a rejecting async disposer rejects the request only after it finishes'
  );

  log = await observe(function (L, gate) {
    return (async function* () {
      await using a = {
        async [Symbol.asyncDispose]() { L('d1-start'); await gate; L('d1-end'); }
      };
      await using b = {
        async [Symbol.asyncDispose]() { L('d2-start'); await null; L('d2-end'); }
      };
      L('body');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'd2-start', 'sync-end', 'd2-end', 'd1-start', 'release', 'd1-end', 'n1:undefined:true'],
    'disposers run in reverse order, each after the previous one settles'
  );

  var later = [];
  var it = (async function* () {
    await using a = { async [Symbol.asyncDispose]() { later.push('d'); await null; } };
    yield 1;
  })();
  var first = await it.next();
  assert.sameValue(first.value, 1, 'first next yields');
  var second = it.next();
  var third = it.next();
  assert.sameValue((await second).done, true, 'second next completes after disposal');
  assert.sameValue((await third).done, true, 'a queued next resolves once the disposal request is done');
  assert.compareArray(later, ['d'], 'the disposer ran exactly once');
});
