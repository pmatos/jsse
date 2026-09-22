// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-generator-function-definitions-runtime-semantics-evaluation
description: >
  Requests queued while an async generator is parked at the `Await` of a later
  `yield*` step settle in FIFO order, each exactly once.
info: |
  AsyncGeneratorEnqueue appends a request to [[AsyncGeneratorQueue]]; a request
  is completed (AsyncGeneratorCompleteStep) and removed exactly once, and the
  next one is only started after the running one has been completed.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration]
---*/

asyncTest(async function () {
  var innerIt = {
    i: 0,
    [Symbol.asyncIterator]() { return this; },
    next() { var v = ++this.i; return Promise.resolve({ value: v, done: false }); },
    return(v) { return Promise.resolve({ value: 'ret:' + v, done: true }); }
  };
  async function* g() { yield* innerIt; }
  var it = g();
  var results = await Promise.all([
    it.next(),
    it.next(),
    it.next(),
    it.return('R'),
    it.next()
  ]);
  assert.compareArray(
    results.map(function (r) { return r.value + ':' + r.done; }),
    ['1:false', '2:false', '3:false', 'ret:R:true', 'undefined:true'],
    'queued requests settle in FIFO order across delegated steps'
  );
});
