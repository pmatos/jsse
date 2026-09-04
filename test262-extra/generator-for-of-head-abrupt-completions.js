/*---
description: >
  Generator for-of head failures discard or close their saved loop state as
  required and remain resumable through enclosing catch and finally clauses.
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
info: |
  ForIn/OfBodyEvaluation propagates failures from IteratorStep and
  IteratorValue without performing IteratorClose. A failure while initializing
  the loop binding instead performs IteratorClose before propagating the
  resulting throw completion through the generator body.
flags: [async]
includes: [compareArray.js]
features: [generators, async-iteration, explicit-resource-management]
---*/

function protocolFailureIterable(kind, events) {
  return {
    [Symbol.iterator]: function () {
      return {
        next: function () {
          if (kind === 'next') {
            throw new Test262Error('next');
          }
          var result = {};
          Object.defineProperty(result, 'done', {
            get: function () {
              if (kind === 'done') {
                throw new Test262Error('done');
              }
              return false;
            },
          });
          Object.defineProperty(result, 'value', {
            get: function () {
              throw new Test262Error('value');
            },
          });
          return result;
        },
        return: function () {
          events.push('return');
          return { value: undefined, done: true };
        },
      };
    },
  };
}

function singleValueIterable(value, events) {
  var done = false;
  return {
    [Symbol.iterator]: function () {
      return {
        next: function () {
          if (done) {
            return { value: undefined, done: true };
          }
          done = true;
          return { value: value, done: false };
        },
        return: function () {
          events.push('return');
          return { value: undefined, done: true };
        },
      };
    },
  };
}

function* catchesProtocolFailure(kind, events) {
  try {
    for (const value of protocolFailureIterable(kind, events)) {
      yield 'unreachable ' + value;
    }
  } catch (error) {
    yield error.message;
  }
  yield 'resumed';
}

for (const kind of ['next', 'done', 'value']) {
  var protocolEvents = [];
  assert.compareArray(
    [...catchesProtocolFailure(kind, protocolEvents)],
    [kind, 'resumed'],
    kind + ' failure is caught and the sync generator resumes'
  );
  assert.compareArray(
    protocolEvents,
    [],
    kind + ' failure does not perform IteratorClose'
  );
}

function* catchesRegistrationFailure(iterable) {
  try {
    for (using resource of iterable) {
      yield 'unreachable ' + resource;
    }
  } catch (error) {
    yield error instanceof TypeError ? 'caught TypeError' : error.message;
  }
  yield 'resumed';
}

var nonDisposableEvents = [];
assert.compareArray(
  [...catchesRegistrationFailure(singleValueIterable({}, nonDisposableEvents))],
  ['caught TypeError', 'resumed'],
  'a non-disposable resource is caught by the sync generator'
);
assert.compareArray(
  nonDisposableEvents,
  ['return'],
  'resource-registration failure closes its iterator exactly once'
);

var disposeGetterError = new Test262Error('dispose getter');
var throwingDisposeGetter = {};
Object.defineProperty(throwingDisposeGetter, Symbol.dispose, {
  get: function () {
    throw disposeGetterError;
  },
});
var getterEvents = [];
var getterFailure = catchesRegistrationFailure(
  singleValueIterable(throwingDisposeGetter, getterEvents)
);
assert.sameValue(getterFailure.next().value, 'dispose getter');
assert.sameValue(getterFailure.next().value, 'resumed');
assert.sameValue(getterFailure.next().done, true);
assert.compareArray(getterEvents, ['return']);

var finallyEvents = [];
function* registrationFailureThroughFinally() {
  try {
    for (using resource of singleValueIterable({}, finallyEvents)) {
      yield 'unreachable ' + resource;
    }
  } finally {
    yield 'finally';
  }
}

var throughFinally = registrationFailureThroughFinally();
assert.sameValue(throughFinally.next().value, 'finally');
assert.throws(TypeError, function () {
  throughFinally.next();
});
assert.sameValue(throughFinally.next().done, true);
assert.compareArray(finallyEvents, ['return']);

var bindingEvents = [];
function* catchesBindingFailure() {
  try {
    for (const [value] of singleValueIterable(null, bindingEvents)) {
      yield 'unreachable ' + value;
    }
  } catch (error) {
    yield error instanceof TypeError ? 'caught TypeError' : error.message;
  }
  yield 'resumed';
}

assert.compareArray(
  [...catchesBindingFailure()],
  ['caught TypeError', 'resumed'],
  'a binding failure is caught by the sync generator'
);
assert.compareArray(bindingEvents, ['return'], 'binding failure closes exactly once');

async function* catchesAsyncProtocolFailure(kind, events) {
  try {
    for (const value of protocolFailureIterable(kind, events)) {
      yield 'unreachable ' + value;
    }
  } catch (error) {
    yield error.message;
  }
  yield 'resumed';
}

async function* catchesAsyncRegistrationFailure(iterable) {
  try {
    for (using resource of iterable) {
      yield 'unreachable ' + resource;
    }
  } catch (error) {
    yield error instanceof TypeError ? 'caught TypeError' : error.message;
  }
  yield 'resumed';
}

async function* asyncRegistrationFailureThroughFinally(events) {
  try {
    for (using resource of singleValueIterable({}, events)) {
      yield 'unreachable ' + resource;
    }
  } finally {
    yield 'finally';
  }
}

async function* catchesAsyncBindingFailure(events) {
  try {
    for (const [value] of singleValueIterable(null, events)) {
      yield 'unreachable ' + value;
    }
  } catch (error) {
    yield error instanceof TypeError ? 'caught TypeError' : error.message;
  }
  yield 'resumed';
}

async function collectAsync(generator) {
  var values = [];
  for (;;) {
    var result = await generator.next();
    if (result.done) {
      return values;
    }
    values.push(result.value);
  }
}

async function runAsyncChecks() {
  for (const kind of ['next', 'done', 'value']) {
    var protocolEvents = [];
    assert.compareArray(
      await collectAsync(catchesAsyncProtocolFailure(kind, protocolEvents)),
      [kind, 'resumed'],
      kind + ' failure is caught and the async generator resumes'
    );
    assert.compareArray(
      protocolEvents,
      [],
      kind + ' failure does not perform IteratorClose in an async generator'
    );
  }

  var registrationEvents = [];
  assert.compareArray(
    await collectAsync(
      catchesAsyncRegistrationFailure(singleValueIterable({}, registrationEvents))
    ),
    ['caught TypeError', 'resumed'],
    'a non-disposable resource is caught by the async generator'
  );
  assert.compareArray(registrationEvents, ['return']);

  var finallyEvents = [];
  var throughFinally = asyncRegistrationFailureThroughFinally(finallyEvents);
  assert.sameValue((await throughFinally.next()).value, 'finally');
  var finallyError;
  try {
    await throughFinally.next();
  } catch (error) {
    finallyError = error;
  }
  assert.sameValue(finallyError instanceof TypeError, true);
  assert.sameValue((await throughFinally.next()).done, true);
  assert.compareArray(finallyEvents, ['return']);

  var bindingEvents = [];
  assert.compareArray(
    await collectAsync(catchesAsyncBindingFailure(bindingEvents)),
    ['caught TypeError', 'resumed'],
    'a binding failure is caught by the async generator'
  );
  assert.compareArray(bindingEvents, ['return']);
}

runAsyncChecks().then($DONE, $DONE);
