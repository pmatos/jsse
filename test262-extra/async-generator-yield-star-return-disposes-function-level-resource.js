// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-generator-function-definitions-runtime-semantics-evaluation
description: >
  `.return(v)` while an async generator is parked in `yield*` completes the
  delegation with a return completion, which propagates through the body: a
  function-level `await using` (and an open `await using` block) is disposed
  exactly once, before the request settles, whichever way the delegation ends
  (inner `return` reporting done, or no inner `return` method).
info: |
  YieldExpression : yield * AssignmentExpression

  7.c. Else (received is a return completion),
    i. Assert: received is a return completion.
    ii. Let return be ? GetMethod(iterator, "return").
    iii. If return is undefined, then
      1. Set value to ? Await(received.[[Value]]).
      2. Return ReturnCompletion(value).
    iv. Let innerReturnResult be ? Call(return, iterator, « received.[[Value]] »).
    v. If generatorKind is async, set innerReturnResult to ? Await(innerReturnResult).
    [...]
    viii. If done is true, then
      1. Let value be ? IteratorValue(innerReturnResult).
      2. Return ReturnCompletion(value).

  The ReturnCompletion propagates out of the generator body, so the body's
  DisposeResources (proposal-explicit-resource-management, sec-disposeresources)
  runs before AsyncGeneratorStart completes the request (sec-asyncgeneratorstart
  step 4.k).
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management, async-iteration]
---*/

function drainTicks(n) {
  var p = Promise.resolve();
  for (var i = 0; i < n; i++) { p = p.then(function () {}); }
  return p;
}

function innerWithReturn() {
  return {
    [Symbol.asyncIterator]() { return this; },
    next() { return Promise.resolve({ value: 'i1', done: false }); },
    return(v) { return { value: v, done: true }; }
  };
}

function innerWithoutReturn() {
  return {
    [Symbol.asyncIterator]() { return this; },
    next() { return Promise.resolve({ value: 'i1', done: false }); }
  };
}

async function run(makeGen) {
  var log = [];
  var L = function (entry) { log.push(entry); };
  var release;
  var gate = new Promise(function (resolve) { release = resolve; });
  var it = makeGen(L, gate);
  var first = await it.next();
  L('first:' + first.value);
  var ret = it.return('R');
  ret.then(function (r) { L('ret:' + r.value + ':' + r.done); });
  var nxt = it.next();
  nxt.then(function (r) { L('next:' + r.value + ':' + r.done); });
  await drainTicks(20);
  var before = log.slice();
  release();
  await ret;
  await nxt;
  await drainTicks(4);
  return { before: before, log: log };
}

asyncTest(async function () {
  var r = await run(function (L) {
    return (async function* () {
      await using a = { [Symbol.asyncDispose]() { L('disp'); } };
      yield* innerWithReturn();
    })();
  });
  assert.compareArray(
    r.log,
    ['first:i1', 'disp', 'ret:R:true', 'next:undefined:true'],
    'inner return reports done: the function-level resource is disposed before the request settles'
  );

  r = await run(function (L) {
    return (async function* () {
      await using a = { [Symbol.asyncDispose]() { L('disp'); } };
      yield* innerWithoutReturn();
    })();
  });
  assert.compareArray(
    r.log,
    ['first:i1', 'disp', 'ret:R:true', 'next:undefined:true'],
    'no inner return method: the function-level resource is disposed before the request settles'
  );

  r = await run(function (L, gate) {
    return (async function* () {
      await using a = {
        async [Symbol.asyncDispose]() { L('d-start'); await gate; L('d-end'); }
      };
      yield* innerWithReturn();
    })();
  });
  assert.compareArray(
    r.before,
    ['first:i1', 'd-start'],
    'an async disposer holds the request (and the queued next) pending'
  );
  assert.compareArray(
    r.log,
    ['first:i1', 'd-start', 'd-end', 'ret:R:true', 'next:undefined:true'],
    'the request settles after the async disposer, and the queued next after it'
  );

  r = await run(function (L, gate) {
    return (async function* () {
      await using a = {
        async [Symbol.asyncDispose]() { L('d-start'); await gate; L('d-end'); }
      };
      yield* innerWithoutReturn();
    })();
  });
  assert.compareArray(
    r.before,
    ['first:i1', 'd-start'],
    'without an inner return, an async disposer still holds the request pending'
  );
  assert.compareArray(
    r.log,
    ['first:i1', 'd-start', 'd-end', 'ret:R:true', 'next:undefined:true'],
    'the request settles after the async disposer'
  );

  r = await run(function (L) {
    return (async function* () {
      await using a = { [Symbol.asyncDispose]() { L('disp-outer'); } };
      {
        await using b = { [Symbol.asyncDispose]() { L('disp-inner'); } };
        yield* innerWithReturn();
      }
    })();
  });
  assert.compareArray(
    r.log,
    ['first:i1', 'disp-inner', 'disp-outer', 'ret:R:true', 'next:undefined:true'],
    'an open await-using block around the yield* is disposed before the function-level one'
  );

  r = await run(function (L) {
    return (async function* () {
      await using a = { [Symbol.asyncDispose]() { L('disp'); } };
      {
        await using b = { [Symbol.asyncDispose]() { L('disp-inner'); } };
        yield* innerWithoutReturn();
      }
    })();
  });
  assert.compareArray(
    r.log,
    ['first:i1', 'disp-inner', 'disp', 'ret:R:true', 'next:undefined:true'],
    'no inner return: the open block frame and the function-level resource are both disposed'
  );
});
