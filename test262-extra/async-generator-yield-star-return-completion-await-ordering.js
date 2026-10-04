// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-generator-function-definitions-runtime-semantics-evaluation
description: >
  A `.return(v)` parked in `yield*` forms its `ReturnCompletion` with exactly
  the number of `Await`s the two shapes of the delegated-return step call
  for -- no more, no fewer -- before the generator body's own disposal runs:
  the inner-`return`-done shape owes zero further `Await`s (the value was
  already fully processed by `Await(innerReturnResult)` + `IteratorComplete`
  + `IteratorValue`), while the no-`.return()`-method shape owes exactly one
  more `Await(received.[[Value]])` beyond `AsyncGeneratorUnwrapYieldResumption`'s
  own initial Await -- and in neither case does disposal start before the
  completion it is disposing is actually formed.
info: |
  YieldExpression : yield * AssignmentExpression

  7.c.iii. If return is undefined, then
    1. Set value to ? Await(received.[[Value]]).
    2. Return ReturnCompletion(value).
  7.c.iv-viii. [...] innerReturnResult = ? Await(Call(return, iterator, ...));
    [...] If done is true, value = ? IteratorValue(innerReturnResult);
    Return ReturnCompletion(value).

  AsyncGeneratorStart step 4.k's DisposeResources runs once the
  YieldExpression's ReturnCompletion has propagated out of the body -- after
  every Await the completion's own formation calls for, not before.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management, async-iteration]
---*/

function drainTicks(n) {
  var p = Promise.resolve();
  for (var i = 0; i < n; i++) { p = p.then(function () {}); }
  return p;
}

asyncTest(async function () {
  // Inner-done-return shape: the delegate's own return() already reports
  // done, so the value is fully processed before ReturnCompletion forms.
  // Disposal must start right after that -- not after one more Await.
  {
    var log = [];
    var innerWithReturn = {
      [Symbol.asyncIterator]() { return this; },
      next() { return Promise.resolve({ value: 'i1', done: false }); },
      return(v) { return Promise.resolve({ value: v, done: true }); }
    };
    var it = (async function* () {
      await using a = {
        [Symbol.asyncDispose]() { log.push('disp'); }
      };
      yield* innerWithReturn;
    })();

    await it.next();
    log.push('got-i1');
    var p = it.return('R');

    var tickCount = 0;
    function countTick() {
      tickCount++;
      log.push('tick' + tickCount);
      if (tickCount < 3) Promise.resolve().then(countTick);
    }
    Promise.resolve().then(countTick);

    var ret = await p;
    log.push('ret:' + ret.value + ':' + ret.done);

    assert.compareArray(
      log,
      ['got-i1', 'tick1', 'disp', 'tick2', 'tick3', 'ret:R:true'],
      'inner-done-return: dispose runs right after tick1 and the request ' +
      'settles with no further tick after dispose (no spurious extra Await)'
    );
  }

  // No-.return()-method shape: AsyncGeneratorUnwrapYieldResumption already
  // Awaited the operand once; this step owes it exactly one more Await
  // before forming ReturnCompletion, and dispose must start after that
  // second Await settles, not before it.
  {
    var log = [];
    var innerWithoutReturn = {
      [Symbol.asyncIterator]() { return this; },
      next() { return Promise.resolve({ value: 'i1', done: false }); }
    };
    var it = (async function* () {
      await using a = {
        async [Symbol.asyncDispose]() {
          log.push('d-start');
          await null;
          log.push('d-end');
        }
      };
      yield* innerWithoutReturn;
    })();

    await it.next();
    log.push('got-i1');
    var p = it.return('R');
    Promise.resolve().then(function () { log.push('marker'); });
    p.then(function (r) { log.push('ret:' + r.value + ':' + r.done); });

    await drainTicks(8);

    assert.compareArray(
      log,
      ['got-i1', 'marker', 'd-start', 'd-end', 'ret:R:true'],
      'no .return() method: the second Await(received.[[Value]]) settles ' +
      '(marker fires) before dispose starts, not after'
    );
  }
});
