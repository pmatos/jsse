// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-disposeresources
description: >
  A yield-free `await using` block in an async generator suspends the
  generator at each Await of DisposeResources; the request settles only once
  a gate-controlled disposer has finished, and the statement after the block
  runs after it.
info: |
  Block : { StatementList }

  DisposeResources (proposal-explicit-resource-management,
  sec-disposeresources) awaits async disposers, suspending the running
  execution context rather than running other jobs inline.
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
    {
      await using a = {
        async [Symbol.asyncDispose]() { L('d-start'); await gate; L('d-end'); }
      };
      L('body');
    }
    L('post');
    yield 'after';
  })();
  var first = it.next();
  first.then(function (r) { L('n1:' + r.value); });
  L('sync-end');
  var drain = Promise.resolve();
  for (var i = 0; i < 12; i++) { drain = drain.then(function () {}); }
  await drain;
  assert.compareArray(
    log,
    ['body', 'd-start', 'sync-end'],
    'the block disposer runs synchronously and the generator suspends at its Await'
  );
  release();
  await first;
  assert.compareArray(
    log,
    ['body', 'd-start', 'sync-end', 'd-end', 'post', 'n1:after'],
    'the statement after the block runs after the disposer finished'
  );
});
