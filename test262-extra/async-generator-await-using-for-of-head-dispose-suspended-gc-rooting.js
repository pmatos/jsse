// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  While a `for (await using x of iterable)` head's per-iteration
  DisposeResources is parked at an Await in an async generator, the iterator,
  the iteration environment's resources, the generator and the in-flight
  request's promise capability all stay reachable across a garbage collection.
info: |
  ForIn/OfBodyEvaluation ( lhs, stmt, iteratorRecord, iterationKind, lhsKind, labelSet [ , iteratorKind ] )

  9.k.ii. Set status to Completion(DisposeResources(iterationEnv.[[DisposeCapability]], result)).

  DisposeResources ( disposeCapability, completion )

  3. For each element resource of disposeCapability.[[DisposableResourceStack]], in reverse list order, do
     [...]
     e. If method is not undefined, then
        i. Let result be Completion(Call(method, value)).
        ii. If result is a normal completion and hint is async-dispose, then
            1. Set result to Completion(Await(result.[[Value]])).
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management, async-iteration, host-gc-required]
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

  var g = (async function* () {
    for (await using a of [makeResource('a'), makeResource('b')]) {
      iterated.push(a.marker);
      log.push('body-' + a.marker);
    }
    log.push('after');
    return 'done';
  })();
  var result = await g.next();

  assert.sameValue(result.value, 'done', 'the request settles with the generator result');
  assert.sameValue(result.done, true, 'and completes the generator');
  assert.compareArray(iterated, ['a', 'b'], 'both iterations bind the resource produced by the iterator');
  assert.compareArray(
    log,
    [
      'body-a', 'disp-start-a', 'disp-end-a',
      'body-b', 'disp-start-b', 'disp-end-b',
      'after',
    ],
    'each iteration disposes before the next runs, surviving a gc while parked'
  );

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

  var failing = (async function* () {
    for (await using a of [makeFailingResource('x')]) {
      errorLog.push('body-' + a.marker);
    }
  })();
  try {
    await failing.next();
    assert(false, 'expected the disposal error to propagate');
  } catch (e) {
    assert.sameValue(e instanceof RangeError, true, 'error identity survives gc');
    assert.sameValue(e.message, 'failing-x', 'error message survives gc');
  }
  assert.compareArray(errorLog, ['body-x', 'disp-start-x'], 'the throwing disposer ran before the error surfaced');

  var settled = [];
  var queued = (async function* () {
    for (await using a of [makeResource('q')]) {
      yield 'y';
    }
    return 'end';
  })();
  var first = queued.next();
  var second = queued.next();
  var third = queued.next();
  settled.push((await first).value);
  settled.push((await second).value);
  settled.push((await third).done);
  assert.compareArray(settled, ['y', 'end', true], 'queued requests survive gc while the head disposal is parked');
});
