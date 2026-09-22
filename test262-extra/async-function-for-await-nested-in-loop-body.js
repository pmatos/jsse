// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  A `for await` nested inside a synchronous loop's body still suspends at
  its per-iteration Await(nextResult). This container isn't reached by a
  top-level "is this statement itself a for-await" check, only by
  recursing into the outer loop's own body.
info: |
  ForIn/OfBodyEvaluation ( lhs, stmt, iteratorRecord, iterationKind, lhsKind, labelSet )

  Repeat,
    a. Let nextResult be ? Call(iteratorRecord.[[NextMethod]], iteratorRecord.[[Iterator]]).
    b. If iteratorKind is async, set nextResult to ? Await(nextResult).
    ...

  ForOfBodyEvaluation just evaluates the loop's own Statement on every entry;
  a nested `for await` inside that statement is subject to the same
  ForIn/OfBodyEvaluation Await as if it were written at the top level.
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

  function mk(tag) {
    var n = 0;
    return {
      [Symbol.asyncIterator]() {
        return {
          next() {
            L(tag + 'next' + (n + 1));
            return Promise.resolve().then(function () {
              return n >= 2 ? { done: true } : { done: false, value: ++n };
            });
          },
        };
      },
    };
  }

  async function f() {
    for (const x of [1, 2]) {
      for await (const y of mk(x + '.')) {
        L('body' + x + '.' + y);
      }
    }
  }

  var p = f();
  L('after-call');

  await tail;
  await p;

  assert.sameValue(
    log.join(','),
    '1.next1,after-call,w1,w2,body1.1,1.next2,w3,w4,body1.2,1.next3,w5,w6,' +
      '2.next1,w7,w8,body2.1,2.next2,body2.2,2.next3',
    'a for-await nested in an outer sync loop body suspends each iteration ' +
      'instead of draining the queue inline'
  );
});
