// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-try-statement-runtime-semantics-evaluation
description: >
  An `await using` declared directly in a `try` block of an async generator
  is disposed when the block is left, before the `finally` block runs, even
  when the generator suspends at a `yield` inside the block.
info: |
  TryStatement : try Block Finally

  1. Let B be Completion(Evaluation of Block).
  2. Let F be Completion(Evaluation of Finally).

  Evaluating Block performs DisposeResources on its completion
  (proposal-explicit-resource-management, sec-disposeresources), so it
  precedes evaluating Finally.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management, async-iteration]
---*/

asyncTest(async function () {
  var log = [];
  var L = function (entry) { log.push(entry); };
  var it = (async function* () {
    try {
      await using a = { async [Symbol.asyncDispose]() { L('d'); } };
      L('body');
      yield 1;
      L('after-yield');
    } finally {
      L('f');
    }
    L('post');
  })();
  await it.next();
  await it.next();
  assert.compareArray(
    log,
    ['body', 'after-yield', 'd', 'f', 'post'],
    'try-block resource disposes before finally, exactly once'
  );

  log = [];
  var it2 = (async function* () {
    try {
      throw 'boom';
    } catch (e) {
      await using a = { async [Symbol.asyncDispose]() { L('d-catch'); } };
      L('catch-body');
      yield 2;
    } finally {
      L('f2');
    }
  })();
  await it2.next();
  await it2.next();
  assert.compareArray(
    log,
    ['catch-body', 'd-catch', 'f2'],
    'catch-clause resource disposes before finally'
  );

  log = [];
  var it3 = (async function* () {
    try {
      await using a = { async [Symbol.asyncDispose]() { L('d3'); } };
      yield 3;
      L('not-reached');
    } finally {
      L('f3');
    }
  })();
  await it3.next();
  var ret = await it3.return('r');
  assert.sameValue(ret.value, 'r', '.return() value is preserved');
  assert.compareArray(log, ['d3', 'f3'], '.return() at a yield disposes the try-block resource before finally');
});
