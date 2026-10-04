// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  An `await` inside a `for-of` head's *assignment*-form object pattern default
  (`for ({a = await x} of outer) {}`, no `let`/`const`/`var`) genuinely
  suspends the enclosing async generator at that `Await`. While parked there
  the generator is still `executing`, so a concurrent `.return()` only
  enqueues (%AsyncGeneratorPrototype%.return step 6) and the outer
  iterable's own iterator is not closed; once the head's `Await` actually
  settles and the loop reaches a real `yield`, the queued `.return()` is
  delivered there, closing the outer iterator exactly once.
info: |
  ForIn/OfBodyEvaluation ( lhs, stmt, iteratorRecord, iterationKind, lhsKind, labelSet )

  [...]
  g. If lhsKind is assignment, then
    i. Set status to Completion(DestructuringAssignmentEvaluation of
       assignmentPattern with argument nextValue).

  %AsyncGeneratorPrototype%.return ( value )

  3. Let state be generator.[[AsyncGeneratorState]].
  [...]
  6. Else,
    a. Assert: state is either executing or draining-queue.
  7. Return promiseCapability.[[Promise]].
flags: [async]
includes: [asyncHelpers.js]
features: [async-iteration, destructuring-assignment]
---*/

function makeCountingIterable(values) {
  var returnCalls = 0;
  return {
    returnCalls: function () {
      return returnCalls;
    },
    [Symbol.iterator]: function () {
      var i = 0;
      return {
        next: function () {
          return i < values.length ? { value: values[i++], done: false } : { value: undefined, done: true };
        },
        return: function (v) {
          returnCalls++;
          return { done: true, value: v };
        },
      };
    },
  };
}

function flush() {
  var p = Promise.resolve();
  for (var i = 0; i < 10; i++) p = p.then(function () {});
  return p;
}

asyncTest(async function () {
  // While genuinely parked at the head's Await, a concurrent .return() only
  // enqueues; the outer iterator must not be closed.
  var a;
  var outer1 = makeCountingIterable([{}]);
  async function* g1() {
    for ({ a = await new Promise(function () {}) } of outer1) {
      yield a;
    }
  }
  var gen1 = g1();
  var nextSettled = false;
  var returnSettled = false;
  gen1.next().then(
    function () { nextSettled = true; },
    function () { nextSettled = true; }
  );
  gen1.return('unused').then(
    function () { returnSettled = true; },
    function () { returnSettled = true; }
  );
  await flush();
  assert.sameValue(nextSettled, false, 'the in-flight next() stays pending behind the stuck head Await');
  assert.sameValue(returnSettled, false, '.return() during the pending head Await only enqueues');
  assert.sameValue(outer1.returnCalls(), 0, 'the outer iterator is not closed while still parked');

  // Once the head's Await settles and the loop reaches a real yield, the
  // queued return() is delivered there, closing the outer iterator exactly
  // once.
  var b;
  var outer2 = makeCountingIterable([{}]);
  var release;
  var pending = new Promise(function (resolve) {
    release = resolve;
  });
  async function* g2() {
    for ({ b = await pending } of outer2) {
      yield b;
    }
  }
  var gen2 = g2();
  var nextResult = gen2.next();
  var returnResult = gen2.return('done-value');
  await flush();
  assert.sameValue(outer2.returnCalls(), 0, 'still parked: the outer iterator has not been closed yet');

  release(42);
  var next = await nextResult;
  assert.sameValue(next.value, 42, 'the released value flows through the head default to the body yield');
  assert.sameValue(next.done, false);
  var ret = await returnResult;
  assert.sameValue(ret.value, 'done-value', 'the queued return() completes with its own value');
  assert.sameValue(ret.done, true);
  assert.sameValue(
    outer2.returnCalls(),
    1,
    'the outer iterator is closed exactly once, once the loop is actually abandoned'
  );
});
