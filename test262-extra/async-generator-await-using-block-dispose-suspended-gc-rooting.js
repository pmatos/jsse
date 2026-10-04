// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-disposeresources
description: >
  While an async generator's block-scope DisposeResources, started by the
  resumption after a `yield`, is parked at an Await, the resources still to be
  disposed, the generator and the in-flight request stay reachable across a
  garbage collection run by a disposer.
info: |
  Block : { StatementList }

  DisposeResources (proposal-explicit-resource-management,
  sec-disposeresources) runs when the block's evaluation completes, and each
  async disposer's result is awaited before the next resource is disposed.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management, async-iteration, host-gc-required]
---*/

asyncTest(async function () {
  var log = [];

  var it = (async function* () {
    {
      await using tail = {
        marker: 'tail',
        async [Symbol.asyncDispose]() { log.push('dispose-' + this.marker); }
      };
      await using collector = {
        async [Symbol.asyncDispose]() {
          await 0;
          $262.gc();
          await 0;
        }
      };
      yield { marker: 'first-' + 'value' };
    }
    yield { marker: 'second-' + 'value' };
  })();

  var first = await it.next();
  assert.sameValue(first.value.marker, 'first-value', 'the value yielded inside the block');
  assert.compareArray(log, [], 'the block scope is still open at the yield');

  var second = await it.next();
  assert.sameValue(second.value.marker, 'second-value', 'the value yielded after the block scope closed');
  assert.compareArray(
    log,
    ['dispose-tail'],
    'the resource still pending when the collection ran was disposed exactly once'
  );
});
