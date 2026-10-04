// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  A `yield` inside a `for await` loop's *assignment*-form head pattern
  (`for await ([x = yield 'd'] of it)`, not a declaration) that actually
  fires resumes the same iteration rather than replaying it or re-acquiring
  the async iterator -- the same "compiled once, replayed never" guarantee
  #753/PR #760 already established for the declaration-form sibling
  (`for (var { a = yield } of it)`), now extended to the assignment form.
info: |
  ForIn/OfBodyEvaluation ( lhs, stmt, iteratorRecord, iterationKind, lhsKind, labelSet [ , iteratorKind ] )

  [...]
  6. Repeat,
    a. Let nextResult be ? Call(iteratorRecord.[[NextMethod]], iteratorRecord.[[Iterator]]).
    b. If iteratorKind is async, set nextResult to ? Await(nextResult).
    [...]
    f. If lhsKind is either assignment or varBinding, then
      i. If lhsKind is assignment, then
        1. Let status be Completion(DestructuringAssignmentEvaluation of
           assignmentPattern with argument nextValue).

  `GetIterator` (called once, from `ForOfInit`) and each iterator `next()`
  call are counted directly: a correct implementation acquires the iterator
  exactly once and steps it exactly once per element, whether or not that
  element's head-pattern default suspends.
flags: [async]
includes: [compareArray.js]
features: [async-iteration, destructuring-assignment]
---*/

async function run() {
  var getIteratorCalls = 0;
  var nextCalls = 0;
  function mkIterable() {
    var n = 0;
    return {
      [Symbol.asyncIterator]() {
        getIteratorCalls++;
        return {
          next() {
            nextCalls++;
            n++;
            // Element 1 is empty (triggers the array-pattern default);
            // element 2 already has a value (default is skipped).
            return Promise.resolve(n > 2 ? { done: true } : { done: false, value: n === 1 ? [] : [n] });
          }
        };
      }
    };
  }

  async function* g() {
    var vals = [];
    var x;
    for await ([x = yield 'default'] of mkIterable()) { vals.push(x); }
    return vals;
  }

  var it = g();
  var r1 = await it.next();
  var r2 = await it.next('sent');
  var r3 = await it.next();

  return { r1, r2, r3, getIteratorCalls, nextCalls };
}

run()
  .then(function (result) {
    assert.sameValue(result.r1.value, 'default', 'the head pattern default suspends on the first element');
    assert.sameValue(result.r1.done, false, 'generator has not completed after the default suspends');
    assert.sameValue(result.r2.done, true, 'generator runs to completion once no further default fires');
    assert.compareArray(result.r2.value, ['sent', 2], 'resumed value and the second, default-free element both land');
    assert.sameValue(result.r3.done, true, 'generator stays completed after its return value is consumed');
    assert.sameValue(result.getIteratorCalls, 1, 'the async iterator is acquired exactly once');
    assert.sameValue(result.nextCalls, 3, 'next() is called exactly once per element plus the exhausting call');
  })
  .then($DONE, $DONE);
