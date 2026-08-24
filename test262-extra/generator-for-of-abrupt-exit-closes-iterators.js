/*---
description: >
  A break or a throwing disposer that leaves a generator for-of closes the
  iterator, disposes the iteration environment, and leaves the generator
  resumable through its own handlers.
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
info: |
  ForIn/OfBodyEvaluation step 7.m: if the loop body produces an abrupt
  completion, return ? IteratorClose(iteratorRecord, status). The iterator's
  `return` method runs as ordinary user code, so it may read and write the
  generator's own bindings while the loop is being closed.

  Step 7.h wraps each iteration in DisposeResources(iterationEnv, result). A
  disposer that throws turns the iteration into a throw completion, which the
  generator's own try statements observe before it escapes to the caller.
includes: [compareArray.js]
features: [generators, explicit-resource-management]
---*/

// The iterator's `return` method writes a binding of the generator that is
// closing it.
function* breaksOutOfLoop() {
  var closed = 'not closed';
  var iterable = {
    [Symbol.iterator]: function () {
      return {
        next: function () {
          return { value: 1, done: false };
        },
        return: function () {
          closed = 'closed';
          return { done: true };
        },
      };
    },
  };

  for (const value of iterable) {
    yield value;
    break;
  }

  yield closed;
  yield typeof value;
}

assert.compareArray(
  [...breaksOutOfLoop()],
  [1, 'closed', 'undefined'],
  'break closes the iterator and discards the iteration binding'
);

// A throwing disposer is a throw completion of the loop, not of the caller.
function* disposerThrows() {
  const resources = [
    {
      [Symbol.dispose]: function () {
        throw new Test262Error('disposer');
      },
    },
    { [Symbol.dispose]: function () {} },
  ];

  try {
    for (using resource of resources) {
      yield 'iteration';
    }
  } catch (error) {
    yield error instanceof Test262Error ? 'caught' : 'wrong error';
  }

  yield 'resumed';
}

assert.compareArray(
  [...disposerThrows()],
  ['iteration', 'caught', 'resumed'],
  'a throwing disposer is caught by the generator and the generator resumes'
);

// With no catch, the disposer's error still runs the generator's `finally`
// and then escapes to the caller.
function* disposerThrowsPastFinally() {
  const resources = [
    {
      [Symbol.dispose]: function () {
        throw new Test262Error('disposer');
      },
    },
    { [Symbol.dispose]: function () {} },
  ];

  try {
    for (using resource of resources) {
      yield 'iteration';
    }
  } finally {
    yield 'finally';
  }

  yield 'unreachable';
}

var seen = [];
assert.throws(
  Test262Error,
  function () {
    for (const value of disposerThrowsPastFinally()) {
      seen.push(value);
    }
  },
  'the disposer error escapes once the generator finally block completes'
);
assert.compareArray(
  seen,
  ['iteration', 'finally'],
  'the finally block runs before the disposer error escapes'
);
