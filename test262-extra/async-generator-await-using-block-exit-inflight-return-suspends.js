// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgeneratorstart
description: >
  Leaving an `await using` block while a return is in flight (a `return`
  statement, or `.return(v)` at a yield inside the block, unwinding through an
  enclosing `try/finally`) suspends the generator at each Await of the block's
  DisposeResources instead of draining the job queue inline. A disposer that
  throws replaces the in-flight return, and the finally selected for the
  return still runs exactly once.
info: |
  Leaving the block runs its DisposeResources
  (proposal-explicit-resource-management, sec-disposeresources), whose Await of
  each async disposer's result suspends the async generator's evaluation
  (sec-await): the enclosing `finally` runs, and the request settles, in later
  jobs than the synchronous caller. A throw completion from a disposer replaces
  the in-flight return completion (DisposeResources step 3.e.iii.2).
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
          return 5;
        }
      } finally {
        L('fin');
      }
    })();
  }, function (it) { return it.next(); });
  assert.compareArray(
    log,
    ['sync-mid', 'dispA', 'w1', 'fin', 'w2', 'res:5:true', 'w3', 'w4', 'w5', 'w6', 'w7', 'w8'],
    'return statement: the disposer and the finally run after the synchronous caller'
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
  }, function (it) { return it.return('R'); });
  assert.compareArray(
    log,
    ['sync-mid', 'dispA', 'w1', 'fin', 'w2', 'res:R:true', 'w3', 'w4', 'w5', 'w6', 'w7', 'w8'],
    '.return() at the yield: the disposer and the finally run after the synchronous caller'
  );

  log = await observe(function (L) {
    return (async function* () {
      try {
        {
          await using a = { [Symbol.asyncDispose]() { L('dispA'); throw 'EA'; } };
          yield 1;
          return 5;
        }
      } finally {
        L('fin');
      }
    })();
  }, function (it) { return it.next(); });
  assert.compareArray(
    log,
    ['sync-mid', 'dispA', 'fin', 'w1', 'rej:EA', 'w2', 'w3', 'w4', 'w5', 'w6', 'w7', 'w8'],
    'a throwing disposer replaces the in-flight return; the finally runs once'
  );

  log = await observe(function (L) {
    return (async function* () {
      try {
        try {
          {
            await using a = { [Symbol.asyncDispose]() { L('dispA'); throw 'EA'; } };
            yield 1;
          }
        } finally {
          L('fin');
        }
      } catch (e) {
        L('outer-caught:' + e);
        yield 'c';
      }
    })();
  }, function (it) { return it.return('R'); });
  assert.compareArray(
    log,
    ['sync-mid', 'dispA', 'fin', 'outer-caught:EA', 'w1', 'w2', 'res:c:false', 'w3', 'w4', 'w5', 'w6', 'w7', 'w8'],
    'the replacing throw is caught by an outer catch after the finally ran once'
  );

  var it = (async function* () {
    try {
      {
        await using a = { async [Symbol.asyncDispose]() { await null; } };
        yield 1;
        return 5;
      }
    } finally {
      await null;
    }
  })();
  await it.next();
  var returned = it.next();
  var queued = it.next();
  assert.sameValue((await returned).value, 5, 'the return statement completes with its value');
  assert.sameValue((await queued).done, true, 'a queued next stays behind the disposing request');
});
