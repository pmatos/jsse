// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgeneratorstart
description: >
  Leaving an `await using` block while a throw is in flight (a `.throw(v)` at a
  yield inside the block) suspends the generator at each Await of the block's
  DisposeResources instead of draining the job queue inline: the code that
  observes the throw runs in a later job than the synchronous caller.
info: |
  AsyncGenerator.prototype.throw ( exception ) resumes the generator with a
  throw completion at its yield. Leaving the block runs its DisposeResources
  (proposal-explicit-resource-management, sec-disposeresources), whose Await of
  each async disposer's result suspends the async generator's evaluation
  (sec-await), so the enclosing `catch` runs only after the Await settles.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management, async-iteration]
---*/

function witnesses(L) {
  var p = Promise.resolve();
  for (var i = 1; i <= 8; i++) {
    (function (n) { p = p.then(function () { L('w' + n); }); })(i);
  }
  return p;
}

async function observe(makeGen, act) {
  var log = [];
  var L = function (entry) { log.push(entry); };
  var it = makeGen(L);
  await it.next();
  var settled = act(it);
  settled.then(
    function (r) { L('res:' + r.value + ':' + r.done); },
    function (e) { L('rej:' + e); }
  );
  L('sync-mid');
  witnesses(L);
  await new Promise(function (resolve) {
    var p = Promise.resolve();
    for (var i = 0; i < 20; i++) { p = p.then(function () {}); }
    p.then(resolve);
  });
  return log;
}

asyncTest(async function () {
  var log = await observe(function (L) {
    return (async function* () {
      try {
        {
          await using a = { [Symbol.asyncDispose]() { L('dispA'); } };
          yield 1;
        }
      } catch (e) {
        L('caught:' + e);
      }
    })();
  }, function (it) { return it.throw('T'); });
  assert.compareArray(
    log,
    ['dispA', 'sync-mid', 'caught:T', 'w1', 'res:undefined:true', 'w2', 'w3', 'w4', 'w5', 'w6', 'w7', 'w8'],
    'the catch around the block runs after the disposer Await, not inside the throw() call'
  );

  log = await observe(function (L) {
    return (async function* () {
      try {
        {
          await using a = { [Symbol.asyncDispose]() { L('dispA'); } };
          {
            await using b = { [Symbol.asyncDispose]() { L('dispB'); throw 'EB'; } };
            yield 1;
          }
        }
      } catch (e) {
        L('caught:' + e.constructor.name + ':' + e.error + ':' + e.suppressed);
      }
    })();
  }, function (it) { return it.throw('T'); });
  assert.compareArray(
    log,
    [
      'dispB', 'dispA', 'sync-mid', 'caught:SuppressedError:EB:T',
      'w1', 'res:undefined:true', 'w2', 'w3', 'w4', 'w5', 'w6', 'w7', 'w8'
    ],
    'nested frames dispose innermost first; a throwing disposer chains onto the in-flight throw'
  );

  log = await observe(function (L) {
    return (async function* () {
      try {
        {
          await using a = { [Symbol.asyncDispose]() { L('dispA'); } };
          yield 1;
        }
      } finally {
        L('fin');
      }
    })();
  }, function (it) { return it.throw('T'); });
  assert.compareArray(
    log,
    ['dispA', 'sync-mid', 'fin', 'w1', 'rej:T', 'w2', 'w3', 'w4', 'w5', 'w6', 'w7', 'w8'],
    'a finally selected for the throw runs once, after the disposer Await'
  );

  var it = (async function* () {
    try {
      {
        await using a = { async [Symbol.asyncDispose]() { await null; } };
        yield 1;
      }
    } catch (e) {
      yield 'c:' + e;
    }
  })();
  await it.next();
  var thrown = it.throw('T');
  var queued = it.next();
  assert.sameValue((await thrown).value, 'c:T', 'the throw resumes the catch after disposal');
  assert.sameValue((await queued).done, true, 'a queued next stays behind the disposing request');
});
