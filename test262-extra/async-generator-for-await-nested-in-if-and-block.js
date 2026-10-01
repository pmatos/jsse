// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  A `for await` nested inside an `if` consequent, or inside a bare block, of
  an async generator still suspends at its per-iteration Await(nextResult)
  instead of draining the microtask queue inline. The `next()` call that
  starts the generator must return control to its caller before the loop's
  later iterations run.
info: |
  ForIn/OfBodyEvaluation ( lhs, stmt, iteratorRecord, iterationKind, lhsKind, labelSet )

  Repeat,
    a. Let nextResult be ? Call(iteratorRecord.[[NextMethod]], iteratorRecord.[[Iterator]]).
    b. If iteratorKind is async, set nextResult to ? Await(nextResult).
    ...

  IfStatement : if ( Expression ) Statement
  Block : { StatementList }

  Both just evaluate their nested Statement(s); neither special-cases or
  suppresses the suspension behavior of a nested for-await.
includes: [asyncHelpers.js]
flags: [async]
features: [async-iteration]
---*/

asyncTest(async function () {
  function mk(tag, log) {
    var n = 0;
    return {
      [Symbol.asyncIterator]() {
        return {
          next() {
            log.push(tag + 'next' + (n + 1));
            return Promise.resolve().then(function () {
              return n >= 2 ? { done: true } : { done: false, value: ++n };
            });
          },
        };
      },
    };
  }

  var logIf = [];
  async function* gIf() {
    if (true) {
      for await (var y of mk('if.', logIf)) {
        logIf.push('body' + y);
      }
    }
  }

  var tailIf = Promise.resolve()
    .then(function () { logIf.push('w1'); })
    .then(function () { logIf.push('w2'); })
    .then(function () { logIf.push('w3'); })
    .then(function () { logIf.push('w4'); });

  var itIf = gIf();
  var doneIf = itIf.next().then(function (r) {
    logIf.push('r:' + JSON.stringify(r));
  });
  logIf.push('after-next');

  var logBlock = [];
  async function* gBlock() {
    {
      for await (var y of mk('block.', logBlock)) {
        logBlock.push('body' + y);
      }
    }
  }

  var tailBlock = Promise.resolve()
    .then(function () { logBlock.push('w1'); })
    .then(function () { logBlock.push('w2'); })
    .then(function () { logBlock.push('w3'); })
    .then(function () { logBlock.push('w4'); });

  var itBlock = gBlock();
  var doneBlock = itBlock.next().then(function (r) {
    logBlock.push('r:' + JSON.stringify(r));
  });
  logBlock.push('after-next');

  await Promise.all([tailIf, doneIf, tailBlock, doneBlock]);

  assert.sameValue(
    logIf.join(','),
    'if.next1,after-next,w1,w2,body1,if.next2,w3,w4,body2,if.next3,r:{"done":true}',
    'a for-await nested in an if consequent suspends each iteration instead of draining ' +
      'the queue inline'
  );
  assert.sameValue(
    logBlock.join(','),
    'block.next1,after-next,w1,w2,body1,block.next2,w3,w4,body2,block.next3,r:{"done":true}',
    'a for-await nested in a bare block suspends each iteration instead of draining the ' +
      'queue inline'
  );
});
