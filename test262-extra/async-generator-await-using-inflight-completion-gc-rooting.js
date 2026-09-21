// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-disposeresources
description: >
  While an async generator disposes a block scope that a throw or a return is
  already crossing, the in-flight throw or return value and the resources still
  to be disposed stay reachable across a garbage collection run by a disposer.
info: |
  DisposeResources ( disposeCapability, completion )

  3. For each element resource of disposeCapability.[[DisposableResourceStack]], in reverse list order, do
     [...]
     e. If method is not undefined, then
        i. Let result be Completion(Call(method, value)).
        ii. If result is a normal completion and hint is async-dispose, then
            1. Set result to Completion(Await(result.[[Value]])).
  4. [...] Return ? completion.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management, async-iteration, host-gc-required]
---*/

function makeCollector() {
  return {
    async [Symbol.asyncDispose]() {
      await 0;
      $262.gc();
      await 0;
    }
  };
}

asyncTest(async function () {
  var log = [];

  var thrown = (async function* () {
    try {
      await using tail = {
        marker: 'tail',
        async [Symbol.asyncDispose]() { log.push('dispose-' + this.marker); }
      };
      await using collector = makeCollector();
      throw { message: 'thrown-' + 'value' };
    } catch (e) {
      log.push('caught-' + e.message);
    }
    yield 1;
  })();
  await thrown.next();
  assert.compareArray(
    log,
    ['dispose-tail', 'caught-thrown-value'],
    'a throw crossing a block scope survives a collection during its disposal'
  );

  log = [];
  var returned = (async function* () {
    try {
      {
        await using tail = {
          marker: 'tail',
          async [Symbol.asyncDispose]() { log.push('dispose-' + this.marker); }
        };
        await using collector = makeCollector();
        return { marker: 'ret-' + 'value' };
      }
    } finally {
      log.push('finally');
    }
  })();
  await returned.next();
  assert.compareArray(
    log,
    ['dispose-tail', 'finally'],
    'a return crossing a block scope completes after a collection during its disposal'
  );
});
