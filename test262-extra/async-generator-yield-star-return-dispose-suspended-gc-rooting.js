// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-disposeresources
description: >
  While an async generator parked in `yield*` is unwinding a `.return(v)`
  through its function-level DisposeResources, suspended at an Await, the
  resources still to be disposed, the returned value, the generator and the
  in-flight request's promise capability stay reachable across a garbage
  collection.
info: |
  DisposeResources ( disposeCapability, completion )

  3. For each element resource of disposeCapability.[[DisposableResourceStack]], in reverse list order, do
     [...]
     e. If method is not undefined, then
        i. Let result be Completion(Call(method, value)).
        ii. If result is a normal completion and hint is async-dispose, then
            1. Set result to Completion(Await(result.[[Value]])).
flags: [async]
includes: [asyncHelpers.js]
features: [explicit-resource-management, async-iteration, host-gc-required]
---*/

function gcTicks(n) {
  var chain = Promise.resolve();
  for (var i = 0; i < n; i++) {
    chain = chain.then(function () { $262.gc(); });
  }
  return chain;
}

function delegate(withReturn) {
  var inner = {
    [Symbol.asyncIterator]() { return this; },
    next() { return Promise.resolve({ value: 'i1', done: false }); }
  };
  if (withReturn) {
    inner.return = function (v) { return { value: v, done: true }; };
  }
  return inner;
}

asyncTest(async function () {
  for (var withReturn of [true, false]) {
    var log = [];
    var gate;
    var pending = new Promise(function (r) { gate = r; });
    var it = (async function* () {
      await using outer = {
        marker: 'outer',
        async [Symbol.asyncDispose]() {
          await pending;
          $262.gc();
          log.push('outer-' + this.marker);
        }
      };
      {
        await using inner = {
          marker: 'inner',
          async [Symbol.asyncDispose]() {
            await 0;
            $262.gc();
            log.push('inner-' + this.marker);
          }
        };
        yield* delegate(withReturn);
      }
    })();
    await it.next();
    var ret = it.return({ marker: 'ret-value' });
    var queued = it.next();
    await gcTicks(6);
    gate();
    var result = await ret;
    assert.sameValue(result.done, true, 'the parked request completes the generator');
    assert.sameValue(result.value.marker, 'ret-value', 'the returned value survives gc');
    assert.sameValue(log.join(), 'inner-inner,outer-outer', 'both parked disposers survive gc and run once');
    var after = await queued;
    assert.sameValue(after.done, true, 'the queued next settles after the disposal');
    assert.sameValue(after.value, undefined, 'the queued next completes with undefined');
  }
});
