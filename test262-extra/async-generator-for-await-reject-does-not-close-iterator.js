// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  When the Await of a `for await` head's next() result rejects inside an async
  generator, the exception propagates without calling the iterator's return()
  method, while enclosing for-of loops it crosses still close normally.
info: |
  ForIn/OfBodyEvaluation ( lhs, stmt, iteratorRecord, iterationKind, lhsKind, labelSet )

  Repeat,
    a. Let nextResult be ? Call(iteratorRecord.[[NextMethod]], iteratorRecord.[[Iterator]]).
    b. If iteratorKind is async, set nextResult to ? Await(nextResult).
    ...

  The `?` on Await(nextResult) returns the abrupt completion straight out of
  ForIn/OfBodyEvaluation; IteratorClose / AsyncIteratorClose are only reached
  from the binding and loop-body steps below it.
includes: [asyncHelpers.js]
flags: [async]
features: [async-iteration]
---*/

asyncTest(async function () {
  var reason = { name: 'rejection reason' };
  var innerReturns = 0;
  var outerReturns = 0;

  function rejectingIterable() {
    return {
      [Symbol.asyncIterator]() {
        return {
          next() {
            return Promise.reject(reason);
          },
          return() {
            innerReturns += 1;
            return {};
          }
        };
      }
    };
  }

  async function* caught() {
    try {
      for await (var x of rejectingIterable()) {
        throw new Test262Error('the body must not run');
      }
    } catch (e) {
      yield e;
    }
    yield 'after';
  }

  var it = caught();
  var first = await it.next();
  assert.sameValue(first.value, reason, 'the catch clause receives the rejection reason');
  assert.sameValue(innerReturns, 0, 'return() is not called for a rejected next() Await');
  var second = await it.next();
  assert.sameValue(second.value, 'after', 'the generator continues after the catch');
  assert.sameValue(innerReturns, 0, 'return() is still not called');

  async function* crossing() {
    var outer = {
      [Symbol.iterator]() {
        return {
          next() {
            return { done: false, value: 1 };
          },
          return() {
            outerReturns += 1;
            return {};
          }
        };
      }
    };
    for (var o of outer) {
      yield 'in-outer';
      for await (var x of rejectingIterable()) {
        throw new Test262Error('the body must not run');
      }
    }
  }

  it = crossing();
  var step = await it.next();
  assert.sameValue(step.value, 'in-outer', 'the generator reaches the outer loop body');
  try {
    await it.next();
    throw new Test262Error('the rejection must propagate out of the generator');
  } catch (e) {
    assert.sameValue(e, reason, 'the generator rejects with the reason');
  }
  assert.sameValue(innerReturns, 0, 'the failed head does not close its own iterator');
  assert.sameValue(outerReturns, 1, 'the crossed enclosing for-of loop still closes');
});
