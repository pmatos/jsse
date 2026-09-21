// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-disposeresources
description: >
  While DisposeResources is parked at an Await, the resources still to be
  disposed and the error accumulated so far stay reachable across a garbage
  collection, for both `await using` and AsyncDisposableStack.disposeAsync.
info: |
  DisposeResources ( disposeCapability, completion )

  3. For each element resource of disposeCapability.[[DisposableResourceStack]], in reverse list order, do
     [...]
     e. If method is not undefined, then
        i. Let result be Completion(Call(method, value)).
        ii. If result is a normal completion and hint is async-dispose, then
            1. Set result to Completion(Await(result.[[Value]])).
            2. Set hasAwaited to true.
        iii. If result is a throw completion, then
             1. If completion is a throw completion, then
                [...] Set completion to ThrowCompletion(error) (a SuppressedError)
             2. Else, set completion to result.
flags: [async]
includes: [asyncHelpers.js]
features: [explicit-resource-management, host-gc-required]
---*/

function makeTail(log) {
  return {
    marker: 'tail',
    async [Symbol.asyncDispose]() {
      log.push('tail-start');
      await 0;
      $262.gc();
      await 0;
      log.push('tail-marker-' + this.marker);
    }
  };
}

asyncTest(async function () {
  var log = [];

  async function viaAwaitUsing() {
    await using tail = makeTail(log);
    await using failing = {
      async [Symbol.asyncDispose]() { throw new RangeError('failing'); }
    };
  }

  try {
    await viaAwaitUsing();
    assert(false, 'expected the await using disposal error to propagate');
  } catch (e) {
    assert.sameValue(e instanceof RangeError, true, 'await using: error identity survives gc');
    assert.sameValue(e.message, 'failing', 'await using: error message survives gc');
  }
  assert.sameValue(log.join(), 'tail-start,tail-marker-tail', 'await using: resource survives gc');

  log = [];
  var outerLog = [];

  async function viaScopeStack() {
    await using outer = {
      marker: 'outer',
      async [Symbol.asyncDispose]() {
        outerLog.push('outer-dispose-' + this.marker);
      }
    };
    {
      await using tail = makeTail(log);
      await using failing = {
        async [Symbol.asyncDispose]() { throw new RangeError('failing'); }
      };
    }
  }

  try {
    await viaScopeStack();
    assert(false, 'expected the nested await using disposal error to propagate');
  } catch (e) {
    assert.sameValue(e instanceof RangeError, true, 'nested scope: error identity survives gc');
    assert.sameValue(e.message, 'failing', 'nested scope: error message survives gc');
  }
  assert.sameValue(
    log.join(),
    'tail-start,tail-marker-tail',
    'nested scope: inner resource survives gc while an outer scope frame is still open'
  );
  assert.sameValue(
    outerLog.join(),
    'outer-dispose-outer',
    'nested scope: the outer scope frame itself still disposes once the inner one is done'
  );

  log = [];
  var stack = new AsyncDisposableStack();
  stack.use(makeTail(log));
  stack.defer(async function () { throw new RangeError('failing'); });
  try {
    await stack.disposeAsync();
    assert(false, 'expected the disposeAsync error to propagate');
  } catch (e) {
    assert.sameValue(e instanceof RangeError, true, 'disposeAsync: error identity survives gc');
    assert.sameValue(e.message, 'failing', 'disposeAsync: error message survives gc');
  }
  assert.sameValue(log.join(), 'tail-start,tail-marker-tail', 'disposeAsync: resource survives gc');
});
