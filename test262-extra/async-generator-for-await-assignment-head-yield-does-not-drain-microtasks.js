// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  A `for await` loop whose *assignment*-form head pattern (`ForInOfLeft::Pattern`,
  not a `var`/`let`/`const` declaration) contains a `yield` -- e.g.
  `for await ([x = yield] of it)` inside an async generator -- suspends the
  running execution context at the loop's own `Await(nextResult)` step
  instead of draining the job queue inline. Before this fix, the statement
  never reached the state-machine transform at all (the per-statement
  suspension detector only recognized a `yield` inside a *declaration* head's
  pattern, not an assignment head's), so it stayed on the tree-walker's
  `exec_for_of_loop`, whose blocking `await_value` fallback ran the rest of
  the first iteration -- including a second `next()` call on the iterator --
  synchronously inside the `.next()` call that should have returned as soon
  as the awaited value settled.
info: |
  ForIn/OfBodyEvaluation ( lhs, stmt, iteratorRecord, iterationKind, lhsKind, labelSet [ , iteratorKind ] )

  [...]
  6. Repeat,
    a. Let nextResult be ? Call(iteratorRecord.[[NextMethod]], iteratorRecord.[[Iterator]]).
    b. If iteratorKind is async, set nextResult to ? Await(nextResult).
    [...]
    e. Let nextValue be ? IteratorValue(nextResult).
    f. If lhsKind is either assignment or varBinding, then
      i. If lhsKind is assignment, then
        1. Let status be Completion(DestructuringAssignmentEvaluation of
           assignmentPattern with argument nextValue).

  Await(value) registers fulfill/reject reactions on the awaited promise and
  returns control to its caller immediately (sec-await) -- it never
  synchronously drains the job queue. A witness chain of promise reactions
  started before the first `next()` call pins how many ticks the loop's own
  `Await` step actually consumes.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration, destructuring-assignment]
---*/

function witnessChain(L, count) {
  var p = Promise.resolve();
  for (var i = 1; i <= count; i++) {
    (function (n) { p = p.then(function () { L('w' + n); }); })(i);
  }
  return p;
}

async function observe(build) {
  var log = [];
  var L = function (entry) { log.push(entry); };
  witnessChain(L, 4);
  var request = build(L);
  request.then(function (r) { L('next-done:' + r.done); });
  L('sync-end');
  var drain = Promise.resolve();
  for (var i = 0; i < 12; i++) { drain = drain.then(function () {}); }
  await drain;
  return log;
}

asyncTest(async function () {
  // Array-pattern assignment head: the default's `yield` never actually
  // fires (the element is never `undefined`), so this isolates the loop's
  // own `Await(nextResult)` step from the pattern-binding step.
  var log = await observe(function (L) {
    var n = 0;
    var iterable = {
      [Symbol.asyncIterator]() {
        return {
          next() {
            L('iter-next' + (n + 1));
            return Promise.resolve(n >= 1 ? { done: true } : { done: false, value: [++n] });
          }
        };
      }
    };
    return (async function* () {
      var x;
      for await ([x = yield] of iterable) { L('body'); }
    })().next();
  });
  assert.compareArray(
    log,
    ['iter-next1', 'sync-end', 'w1', 'body', 'iter-next2', 'w2', 'w3', 'next-done:true', 'w4'],
    'array-pattern assignment head: the loop Await suspends before the caller\'s stack unwinds'
  );

  // Object-pattern assignment head, same shape.
  log = await observe(function (L) {
    var n = 0;
    var iterable = {
      [Symbol.asyncIterator]() {
        return {
          next() {
            L('iter-next' + (n + 1));
            n++;
            return Promise.resolve(n > 1 ? { done: true } : { done: false, value: { a: 1 } });
          }
        };
      }
    };
    return (async function* () {
      var a;
      for await ({ a = yield } of iterable) { L('body-a=' + a); }
    })().next();
  });
  assert.compareArray(
    log,
    ['iter-next1', 'sync-end', 'w1', 'body-a=1', 'iter-next2', 'w2', 'w3', 'next-done:true', 'w4'],
    'object-pattern assignment head: the loop Await suspends before the caller\'s stack unwinds'
  );
});
