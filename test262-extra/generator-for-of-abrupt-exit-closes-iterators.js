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

// IteratorClose failures are abrupt completions of the loop and therefore
// remain inside the generator's own control flow.
function throwingCloseIterable() {
  return {
    [Symbol.iterator]: function () {
      return {
        next: function () {
          return { value: 1, done: false };
        },
        return: function () {
          throw new Test262Error('close');
        },
      };
    },
  };
}

function* catchesCloseError() {
  try {
    for (const value of throwingCloseIterable()) {
      yield value;
      break;
    }
  } catch (error) {
    yield error.message;
  }
  yield 'resumed';
}

assert.compareArray(
  [...catchesCloseError()],
  [1, 'close', 'resumed'],
  'the generator catches an IteratorClose failure and remains resumable'
);

function* doesNotCatchCloseError() {
  for (const value of throwingCloseIterable()) {
    yield value;
    break;
  }
}

var uncaughtClose = doesNotCatchCloseError();
assert.sameValue(uncaughtClose.next().value, 1);
assert.throws(
  Test262Error,
  function () {
    uncaughtClose.next();
  },
  'an uncaught IteratorClose failure escapes'
);
assert.sameValue(
  uncaughtClose.next().done,
  true,
  'an uncaught IteratorClose failure completes rather than wedges the generator'
);

// A labelled continue closes only loops exited by the completion. The target
// loop remains active and advances to its next iteration.
var continueEvents = [];
function trackedContinueIterable(name, values) {
  var index = 0;
  return {
    [Symbol.iterator]: function () {
      return this;
    },
    next: function () {
      return index < values.length
        ? { value: values[index++], done: false }
        : { value: undefined, done: true };
    },
    return: function () {
      continueEvents.push('close ' + name);
      return { value: undefined, done: true };
    },
  };
}

function* labeledContinueKeepsTargetLoop() {
  outer: for (const outerValue of trackedContinueIterable('outer', [1, 2])) {
    for (const innerValue of trackedContinueIterable('inner ' + outerValue, [3])) {
      yield outerValue + innerValue;
      continue outer;
    }
  }
  yield continueEvents.join(',');
}

assert.compareArray(
  [...labeledContinueKeepsTargetLoop()],
  [4, 5, 'close inner 1,close inner 2'],
  'labelled continue closes nested iterators but retains its target iterator'
);

// A return injected at a suspended yield first completes the loop body. An
// inner finally therefore runs before per-iteration disposal and IteratorClose,
// while a finally surrounding the loop runs afterwards.
var returnEvents = [];
var returnResource = {
  [Symbol.dispose]: function () {
    returnEvents.push('dispose');
  },
};
var returnIterable = {
  [Symbol.iterator]: function () {
    return {
      next: function () {
        return { value: returnResource, done: false };
      },
      return: function () {
        returnEvents.push('close');
        return { done: true };
      },
    };
  },
};

function* closesUsingIterationOnReturn() {
  try {
    for (using resource of returnIterable) {
      try {
        yield 'body';
      } finally {
        returnEvents.push('inner finally');
      }
    }
  } finally {
    returnEvents.push('outer finally');
  }
}

var returned = closesUsingIterationOnReturn();
assert.sameValue(returned.next().value, 'body');
var returnResult = returned.return('return value');
assert.sameValue(returnResult.value, 'return value');
assert.sameValue(returnResult.done, true);
assert.compareArray(
  returnEvents,
  ['inner finally', 'dispose', 'close', 'outer finally'],
  'return disposes the active iteration and closes its iterator in spec order'
);
assert.sameValue(returned.next().done, true);

// If abrupt return cleanup throws, that throw replaces the return completion
// and can be handled by a catch surrounding the loop.
var throwingReturnEvents = [];
var throwingReturnResource = {
  [Symbol.dispose]: function () {
    throwingReturnEvents.push('dispose');
    throw new Test262Error('return disposer');
  },
};
var throwingReturnIterable = {
  [Symbol.iterator]: function () {
    return {
      next: function () {
        return { value: throwingReturnResource, done: false };
      },
      return: function () {
        throwingReturnEvents.push('close');
        return { done: true };
      },
    };
  },
};

function* catchesReturnDisposerError() {
  try {
    for (using resource of throwingReturnIterable) {
      yield 'body';
    }
  } catch (error) {
    yield error.message;
  }
  yield 'resumed';
}

var throwingReturn = catchesReturnDisposerError();
assert.sameValue(throwingReturn.next().value, 'body');
var caughtReturnDisposer = throwingReturn.return('ignored return value');
assert.sameValue(caughtReturnDisposer.value, 'return disposer');
assert.sameValue(caughtReturnDisposer.done, false);
assert.sameValue(throwingReturn.next().value, 'resumed');
assert.sameValue(throwingReturn.next().done, true);
assert.compareArray(
  throwingReturnEvents,
  ['dispose', 'close'],
  'a disposer throw still closes the iterator before entering catch'
);
