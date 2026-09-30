// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  A `for await` nested inside a `try` block of an async generator still
  suspends at its per-iteration Await(nextResult) instead of draining the
  microtask queue inline. The `next()` call that starts the generator must
  return control to its caller before the loop's later iterations run, and
  the `finally` block must run only after the loop truly completes. This
  exercises the `is_await` disjunct used by async generators (as opposed to
  the `detect_for_await` disjunct used by async functions), reached only
  via `f.is_await` on the loop's own ForOfStatement head.
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
  to the caller of next() (step 8) before the Await settles. TryStatement's
  Evaluation of its Block just evaluates the nested StatementList; it does
  not special-case or suppress this suspension behavior.
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
    .then(function () { L('w5'); })
    .then(function () { L('w6'); })
    .then(function () { L('w7'); })
    .then(function () { L('w8'); });

  function mk() {
    var n = 0;
    return {
      [Symbol.asyncIterator]() {
        return {
          next() {
            L('next' + (n + 1));
            return Promise.resolve().then(function () {
              return n >= 3 ? { done: true } : { done: false, value: ++n };
            });
          },
        };
      },
    };
  }

  async function* g() {
    try {
      for await (var y of mk()) {
        L('body' + y);
      }
    } finally {
      L('finally');
    }
  }

  var it = g();
  var done = it.next().then(function (r) {
    L('r:' + JSON.stringify(r));
  });
  L('after-next');

  await tail;
  await done;

  assert.sameValue(
    log.join(','),
    'next1,after-next,w1,w2,body1,next2,w3,w4,body2,next3,w5,w6,body3,next4,w7,w8,finally,' +
      'r:{"done":true}',
    'a for-await nested in try suspends each iteration instead of draining the queue inline, ' +
      'and next() returns to its caller before the loop advances'
  );
});
