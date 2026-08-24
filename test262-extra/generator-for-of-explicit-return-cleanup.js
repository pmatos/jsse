/*---
description: >
  Source-level returns from transformed generators dispose active for-using
  bindings and close every exited for-of iterator in spec order.
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
flags: [async]
includes: [compareArray.js]
features: [generators, async-iteration, explicit-resource-management]
---*/

function trackedSingleValue(value, events) {
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
          events.push('close');
          return { value: undefined, done: true };
        },
      };
    },
  };
}

var syncEvents = [];
var syncResource = {
  [Symbol.dispose]: function () {
    syncEvents.push('dispose');
  },
};

function* syncExplicitReturn() {
  try {
    for (using resource of trackedSingleValue(syncResource, syncEvents)) {
      try {
        yield 'body';
        return 42;
      } finally {
        syncEvents.push('inner finally');
      }
    }
  } finally {
    syncEvents.push('outer finally');
  }
}

var syncReturn = syncExplicitReturn();
assert.sameValue(syncReturn.next().value, 'body');
var syncResult = syncReturn.next();
assert.sameValue(syncResult.value, 42);
assert.sameValue(syncResult.done, true);
assert.compareArray(
  syncEvents,
  ['inner finally', 'dispose', 'close', 'outer finally'],
  'sync source return observes finally/dispose/close order'
);

var plainEvents = [];
function* plainExplicitReturn() {
  for (const value of trackedSingleValue(1, plainEvents)) {
    yield value;
    return 7;
  }
}

var plainReturn = plainExplicitReturn();
assert.sameValue(plainReturn.next().value, 1);
assert.sameValue(plainReturn.next().value, 7);
assert.compareArray(plainEvents, ['close'], 'plain sync for-of closes on source return');

var throwingEvents = [];
var throwingResource = {
  [Symbol.dispose]: function () {
    throwingEvents.push('dispose');
    throw new Test262Error('dispose');
  },
};

function* catchesExplicitReturnCleanupError() {
  try {
    for (using resource of trackedSingleValue(throwingResource, throwingEvents)) {
      yield 'body';
      return 9;
    }
  } catch (error) {
    yield error.message;
  }
  yield 'resumed';
}

assert.compareArray(
  [...catchesExplicitReturnCleanupError()],
  ['body', 'dispose', 'resumed'],
  'a source-return cleanup error enters the sync generator catch'
);
assert.compareArray(throwingEvents, ['dispose', 'close']);

async function* asyncExplicitReturn(events, resource) {
  try {
    for (using current of trackedSingleValue(resource, events)) {
      try {
        yield 'body';
        return 42;
      } finally {
        events.push('inner finally');
      }
    }
  } finally {
    events.push('outer finally');
  }
}

async function* asyncPlainExplicitReturn(events) {
  for (const value of trackedSingleValue(1, events)) {
    yield value;
    return 7;
  }
}

async function* catchesAsyncExplicitReturnCleanupError(events, resource) {
  try {
    for (using current of trackedSingleValue(resource, events)) {
      yield 'body';
      return 9;
    }
  } catch (error) {
    yield error.message;
  }
  yield 'resumed';
}

async function runAsyncChecks() {
  var events = [];
  var resource = {
    [Symbol.dispose]: function () {
      events.push('dispose');
    },
  };
  var generator = asyncExplicitReturn(events, resource);
  assert.sameValue((await generator.next()).value, 'body');
  var result = await generator.next();
  assert.sameValue(result.value, 42);
  assert.sameValue(result.done, true);
  assert.compareArray(events, ['inner finally', 'dispose', 'close', 'outer finally']);

  var plainEvents = [];
  var plain = asyncPlainExplicitReturn(plainEvents);
  assert.sameValue((await plain.next()).value, 1);
  assert.sameValue((await plain.next()).value, 7);
  assert.compareArray(plainEvents, ['close']);

  var throwingEvents = [];
  var throwing = {
    [Symbol.dispose]: function () {
      throwingEvents.push('dispose');
      throw new Test262Error('dispose');
    },
  };
  var caught = catchesAsyncExplicitReturnCleanupError(throwingEvents, throwing);
  assert.sameValue((await caught.next()).value, 'body');
  assert.sameValue((await caught.next()).value, 'dispose');
  assert.sameValue((await caught.next()).value, 'resumed');
  assert.sameValue((await caught.next()).done, true);
  assert.compareArray(throwingEvents, ['dispose', 'close']);
}

runAsyncChecks().then($DONE, $DONE);
