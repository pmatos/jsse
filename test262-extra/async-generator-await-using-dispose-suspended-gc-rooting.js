// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-disposeresources
description: >
  While an async generator's function-level DisposeResources is parked at an
  Await, the resources still to be disposed, the error accumulated so far, the
  generator itself and the in-flight request's promise capability stay
  reachable across a garbage collection.
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
features: [explicit-resource-management, async-iteration, host-gc-required]
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

  var it = (async function* () {
    await using tail = makeTail(log);
    await using failing = {
      async [Symbol.asyncDispose]() { throw new RangeError('failing'); }
    };
    yield 1;
  })();
  await it.next();
  try {
    await it.next();
    assert(false, 'expected the disposal error to propagate');
  } catch (e) {
    assert.sameValue(e instanceof RangeError, true, 'error identity survives gc');
    assert.sameValue(e.message, 'failing', 'error message survives gc');
  }
  assert.sameValue(log.join(), 'tail-start,tail-marker-tail', 'resource survives gc');

  log = [];
  var settled = new Promise(function (resolve) {
    (function () {
      var gate;
      var pending = new Promise(function (r) { gate = r; });
      var it2 = (async function* () {
        await using tail = {
          marker: 'held',
          async [Symbol.asyncDispose]() {
            await pending;
            log.push('held-' + this.marker);
          }
        };
        return 'value';
      })();
      it2.next().then(resolve);
      setTimeoutLike(gate);
    })();
  });

  function setTimeoutLike(gate) {
    var chain = Promise.resolve();
    for (var i = 0; i < 4; i++) {
      chain = chain.then(function () { $262.gc(); });
    }
    chain.then(gate);
  }

  var result = await settled;
  assert.sameValue(result.value, 'value', 'the parked request still resolves with its value');
  assert.sameValue(result.done, true, 'and completes the generator');
  assert.sameValue(log.join(), 'held-held', 'the parked disposer still ran after gc');
});
