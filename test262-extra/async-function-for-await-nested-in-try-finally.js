// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  A `for await` nested inside a `try` block of an async function still
  suspends at its per-iteration Await(nextResult), the same as a top-level
  `for await`. The container contributes nothing but completion-record
  plumbing and must not cause the loop to fall back to a blocking native
  execution that only drains already-queued microtasks.
info: |
  ForIn/OfBodyEvaluation ( lhs, stmt, iteratorRecord, iterationKind, lhsKind, labelSet )

  Repeat,
    a. Let nextResult be ? Call(iteratorRecord.[[NextMethod]], iteratorRecord.[[Iterator]]).
    b. If iteratorKind is async, set nextResult to ? Await(nextResult).
    ...

  TryStatement : try Block Finally

  1. Let blockResult be Completion(Evaluation of Block).
  ...

  Evaluation of Block just evaluates its StatementList; it does not special-case
  or suppress the suspension behavior of a nested for-await, so every iteration's
  Await must still return control to the caller of next() before its continuation
  runs as a later queued Job.
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

  async function f() {
    try {
      for await (var x of mk()) {
        L('body' + x);
      }
    } finally {
      L('finally');
    }
  }

  var p = f();
  L('after-call');

  await tail;
  await p;

  assert.sameValue(
    log.join(','),
    'next1,after-call,w1,w2,body1,next2,w3,w4,body2,next3,w5,w6,body3,next4,w7,w8,finally',
    'a for-await nested in try suspends each iteration instead of draining the queue inline, ' +
      'and finally runs only after the loop truly completes'
  );
});
