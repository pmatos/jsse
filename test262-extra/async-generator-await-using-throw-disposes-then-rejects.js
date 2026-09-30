// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgeneratorstart
description: >
  A throw leaving an async generator body disposes the function-level
  `await using` resources (suspending at each Await) before the request
  promise is rejected; a disposer error is chained as a SuppressedError.
info: |
  AsyncGeneratorStart ( generator, generatorBody )

  4. Let result be Completion(Evaluation of generatorBody).
  [...]
  k. Perform AsyncGeneratorCompleteStep(acGenerator, result, true).

  DisposeResources ( disposeCapability, completion ) is part of evaluating the
  body (proposal-explicit-resource-management, sec-disposeresources).
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management, async-iteration]
---*/

asyncTest(async function () {
  var log = [];
  var L = function (entry) { log.push(entry); };
  var release;
  var gate = new Promise(function (resolve) { release = resolve; });
  var it = (async function* () {
    await using a = { async [Symbol.asyncDispose]() { L('d-start'); await gate; L('d-end'); } };
    yield 1;
    throw new Error('boom');
  })();
  await it.next();
  var second = it.next();
  second.then(
    function () { L('resolved'); },
    function (e) { L('rejected:' + e.message); }
  );
  var third = it.next();
  third.then(function (r) { L('n3:' + r.done); });
  var drain = Promise.resolve();
  for (var i = 0; i < 12; i++) { drain = drain.then(function () {}); }
  await drain;
  assert.compareArray(log, ['d-start'], 'the request stays pending while the disposer awaits');
  release();
  await third;
  assert.compareArray(
    log,
    ['d-start', 'd-end', 'rejected:boom', 'n3:true'],
    'rejection is delivered after disposal and the queued next then completes'
  );

  log = [];
  var it2 = (async function* () {
    await using a = { async [Symbol.asyncDispose]() { await null; throw 'from-disposer'; } };
    throw 'original';
  })();
  var caught;
  try { await it2.next(); } catch (e) { caught = e; }
  assert.sameValue(caught.constructor, SuppressedError, 'disposer error chains onto the throw');
  assert.sameValue(caught.error, 'from-disposer', 'error is the disposer error');
  assert.sameValue(caught.suppressed, 'original', 'suppressed is the original throw');

  var it3 = (async function* () {
    await using a = { async [Symbol.asyncDispose]() { L('d3'); await null; } };
    try { throw 'inner'; } catch (e) { L('caught:' + e); }
    yield 'ok';
  })();
  log = [];
  var r3 = await it3.next();
  assert.sameValue(r3.value, 'ok', 'a catch inside the generator still wins');
  assert.compareArray(log, ['caught:inner'], 'disposal has not run yet');
  await it3.next();
  assert.compareArray(log, ['caught:inner', 'd3'], 'disposal runs on completion');
});
