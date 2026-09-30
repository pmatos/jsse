// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgeneratorstart
description: >
  `.return(v)` at a yield unwinds through a function-level `await using`:
  the disposer runs exactly once, and the request settles only after the
  disposer's promise settles, with the returned value.
info: |
  AsyncGenerator.prototype.return ( value ) resumes the generator with a return
  completion; the body's DisposeResources (proposal-explicit-resource-management,
  sec-disposeresources) runs before AsyncGeneratorCompleteStep settles the
  request (sec-asyncgeneratorstart step 4.k).
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
    L('not-reached');
  })();
  await it.next();
  var ret = it.return(9);
  ret.then(function (r) { L('ret:' + r.value + ':' + r.done); });
  var drain = Promise.resolve();
  for (var i = 0; i < 12; i++) { drain = drain.then(function () {}); }
  await drain;
  assert.compareArray(log, ['d-start'], 'the request stays pending while the disposer awaits');
  release();
  await ret;
  assert.compareArray(
    log,
    ['d-start', 'd-end', 'ret:9:true'],
    'the disposer ran once and the request settled after it'
  );
});
