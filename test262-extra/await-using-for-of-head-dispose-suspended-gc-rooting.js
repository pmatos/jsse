// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  While a `for (await using x of iterable)` head's per-iteration
  DisposeResources is parked at an Await, the iterator, the iteration
  environment's resources and the accumulated disposal error all stay
  reachable across a garbage collection.
info: |
  ForIn/OfBodyEvaluation ( lhs, stmt, iterator, iteratorKind, lhsKind, labelSet [ , iteratorRecordLevel ] )

  9.j-k. Set status to Completion(DisposeResources(iterationEnv.[[DisposeCapability]], result)).

  DisposeResources ( disposeCapability, completion )

  3. For each element resource of disposeCapability.[[DisposableResourceStack]], in reverse list order, do
     [...]
     e. If method is not undefined, then
        i. Let result be Completion(Call(method, value)).
        ii. If result is a normal completion and hint is async-dispose, then
            1. Set result to Completion(Await(result.[[Value]])).
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management, host-gc-required]
---*/

asyncTest(async function () {
  var log = [];
  var iterated = [];

  function makeResource(marker) {
    return {
      marker: marker,
      async [Symbol.asyncDispose]() {
        log.push('disp-start-' + this.marker);
        await 0;
        $262.gc();
        await 0;
        log.push('disp-end-' + this.marker);
      }
    };
  }

  async function run() {
    for (await using a of [makeResource('a'), makeResource('b')]) {
      iterated.push(a.marker);
      log.push('body-' + a.marker);
    }
    log.push('after');
  }

  await run();

  assert.compareArray(iterated, ['a', 'b'], 'both iterations bind the resource produced by the iterator');
  assert.compareArray(
    log,
    [
      'body-a',
      'disp-start-a',
      'disp-end-a',
      'body-b',
      'disp-start-b',
      'disp-end-b',
      'after',
    ],
    'each iteration disposes before the next runs, surviving a gc while parked'
  );

  // A disposer's error survives a gc while a later parked Await is pending.
  var errorLog = [];
  function makeFailingResource(marker) {
    return {
      marker: marker,
      async [Symbol.asyncDispose]() {
        errorLog.push('disp-start-' + this.marker);
        await 0;
        $262.gc();
        await 0;
        throw new RangeError('failing-' + this.marker);
      }
    };
  }

  async function runFailing() {
    for (await using a of [makeFailingResource('x')]) {
      errorLog.push('body-' + a.marker);
    }
  }

  try {
    await runFailing();
    assert(false, 'expected the disposal error to propagate');
  } catch (e) {
    assert.sameValue(e instanceof RangeError, true, 'error identity survives gc');
    assert.sameValue(e.message, 'failing-x', 'error message survives gc');
  }
  assert.compareArray(errorLog, ['body-x', 'disp-start-x'], 'the throwing disposer ran before the error surfaced');
});
