// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  `for await` executed directly in an async generator body suspends the
  generator at its per-iteration Await(nextResult) instead of draining the
  microtask queue inline. next() must return control to its caller before
  any of that Await's sibling microtasks run.
info: |
  ForIn/OfBodyEvaluation ( lhs, stmt, iteratorRecord, iterationKind, lhsKind, labelSet )

  Repeat,
    a. Let nextResult be ? Call(iteratorRecord.[[NextMethod]], iteratorRecord.[[Iterator]]).
    b. If iteratorKind is async, set nextResult to ? Await(nextResult).
    ...

  Await ( value )

  5. Perform PerformPromiseThen(promise, onFulfilled, onRejected).
  6. Remove asyncContext from the execution context stack ...
  7. Let callerContext be the running execution context.
  8. Resume callerContext ...
  9. Return undefined.

  Step 6 removes the async generator's execution context and returns control
  to the caller of next() (step 8) before the Await settles; the awaited
  continuation runs only as a later queued Job, not synchronously inline.
includes: [asyncHelpers.js]
flags: [async]
features: [async-iteration]
---*/

asyncTest(async function () {
  var log = [];
  function L(x) {
    log.push(x);
  }

  var tail = Promise.resolve()
    .then(function () { L('w1'); })
    .then(function () { L('w2'); })
    .then(function () { L('w3'); })
    .then(function () { L('w4'); })
    .then(function () { L('w5'); });

  async function* g() {
    L('body');
    for await (var y of [1]) {
      L('y' + y);
    }
  }

  var it = g();
  it.next();
  L('after-next');

  await tail;

  assert.sameValue(
    log.join(','),
    'body,after-next,w1,w2,y1,w3,w4,w5',
    'the for-await head suspends via a Job instead of draining the queue inline'
  );
});
