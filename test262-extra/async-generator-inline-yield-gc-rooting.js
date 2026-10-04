// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-yield
description: >
  While an async generator is parked at the `Await` of an inline `yield` (an
  operand the state-machine transform does not lower) or of an inline `yield*`
  step, the generator, its pending request, the awaited operand and the inner
  iterator stay reachable across a garbage collection even though nothing else
  references them.
info: |
  Yield ( value )

  2. If generatorKind is async, return ? AsyncGeneratorYield(? Await(value)).

  Await suspends the running execution context; the suspended generator, the
  request being processed and the awaited promise's reactions must remain live
  until the promise settles.
flags: [async]
includes: [asyncHelpers.js]
features: [async-iteration, destructuring-assignment, host-gc-required]
---*/

function collectThenRelease(release, value) {
  setTimeout(function () {
    $262.gc();
    setTimeout(function () {
      $262.gc();
      release(value);
    }, 0);
  }, 0);
}

function startYield() {
  return new Promise(function (resolve) {
    var release;
    var gate = new Promise(function (r) { release = r; });
    var it = (async function* () {
      var a;
      ({a = yield gate} = {});
      return 'end:' + a;
    })();
    it.next().then(function (first) {
      resolve(first);
    });
    gate = null;
    collectThenRelease(release, 'operand');
  });
}

function startYieldStar(step) {
  return new Promise(function (resolve) {
    var release;
    var gate = new Promise(function (r) { release = r; });
    var inner = {
      [Symbol.asyncIterator]() { return this; },
      next() {
        return (step === 'first' || this.calls++ > 0) ? gate : { value: 1, done: false };
      },
      calls: 0
    };
    var it = (async function* () {
      var a;
      ({a = yield* inner} = {});
      return 'end:' + a;
    })();
    if (step === 'first') {
      it.next().then(resolve);
      collectThenRelease(release, { value: 'x', done: true });
    } else {
      it.next().then(function () {
        it.next().then(resolve);
        collectThenRelease(release, { value: 'x', done: true });
      });
    }
  });
}

asyncTest(async function () {
  var yielded = await startYield();
  assert.sameValue(yielded.value, 'operand', 'inline yield: the parked request yields the awaited operand after gc');
  assert.sameValue(yielded.done, false, 'inline yield: the generator is not done');

  var steps = ['first', 'later'];
  for (var i = 0; i < steps.length; i++) {
    var result = await startYieldStar(steps[i]);
    assert.sameValue(result.value, 'end:x', 'inline yield* ' + steps[i] + ' step: the parked request resolves after gc');
    assert.sameValue(result.done, true, 'inline yield* ' + steps[i] + ' step: the generator completes');
  }
});
