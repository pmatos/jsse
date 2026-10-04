// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-generator-function-definitions-runtime-semantics-evaluation
description: >
  While an async generator is parked at `Await(innerResult)` of a later
  `yield*` step, the generator, its pending request and the inner iterator stay
  reachable across a garbage collection even though nothing else references
  them.
info: |
  YieldExpression : yield * AssignmentExpression

  Await suspends the running execution context; the suspended generator, the
  request being processed and the awaited promise's reactions must remain live
  until the promise settles.
flags: [async]
includes: [asyncHelpers.js]
features: [async-iteration, host-gc-required]
---*/

function start(kind) {
  return new Promise(function (resolve) {
    var release;
    var gate = new Promise(function (r) { release = r; });
    var inner = {
      [Symbol.asyncIterator]() { return this; },
      next() { return kind === 'next' && this.calls++ > 0 ? gate : { value: 1, done: false }; },
      throw() { return gate; },
      return() { return gate; },
      calls: 0
    };
    var it = (async function* () {
      var r = yield* inner;
      return 'end:' + r;
    })();
    it.next().then(function () {
      var request = kind === 'next' ? it.next() : kind === 'throw' ? it.throw('E') : it.return('R');
      request.then(resolve);
      setTimeout(function () {
        $262.gc();
        setTimeout(function () {
          $262.gc();
          release({ value: 'x', done: true });
        }, 0);
      }, 0);
    });
  });
}

asyncTest(async function () {
  var kinds = ['next', 'throw', 'return'];
  for (var i = 0; i < kinds.length; i++) {
    var result = await start(kinds[i]);
    var expected = kinds[i] === 'return' ? 'x' : 'end:x';
    assert.sameValue(result.value, expected, kinds[i] + ': the parked request resolves with its value after gc');
    assert.sameValue(result.done, true, kinds[i] + ': and completes the generator');
  }
});
