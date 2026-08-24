/*---
description: >
  Generator for-of loops preserve fresh lexical bindings across suspension.
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
info: |
  ForIn/OfBodyEvaluation creates a new declarative environment for every
  lexical iteration, evaluates the loop body in that environment, and restores
  the environment that was active before the loop afterwards.

  Suspending a generator must retain the active iteration environment so that
  closures created by different iterations capture different bindings.
flags: [async]
includes: [compareArray.js]
features: [generators, async-iteration, destructuring-binding]
---*/

function* syncGenerator() {
  for (const value of [1, 2]) {
    yield function () { return value; };
  }

  for (let [value] of [[3], [4]]) {
    yield function () { return value; };
  }
}

var syncClosures = [...syncGenerator()];
assert.compareArray(
  syncClosures.map(function (closure) { return closure(); }),
  [1, 2, 3, 4],
  'sync generator iterations have distinct lexical bindings'
);

function trackedIterable(value, events) {
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

function* syncThrowToOuterCatch(events) {
  try {
    for (const value of trackedIterable(1, events)) {
      yield 'body';
      throw new Test262Error('body');
    }
  } catch (error) {
    events.push('catch');
    yield typeof value;
  }
}

var syncThrowEvents = [];
assert.compareArray(
  [...syncThrowToOuterCatch(syncThrowEvents)],
  ['body', 'undefined'],
  'an outer catch runs outside the exited sync iteration environment'
);
assert.compareArray(
  syncThrowEvents,
  ['return', 'catch'],
  'IteratorClose precedes the outer sync catch'
);

function* syncInjectedThrowToOuterCatch(events) {
  try {
    for (const value of trackedIterable(1, events)) {
      yield 'body';
    }
  } catch (error) {
    events.push('catch');
    yield typeof value;
  }
}

var syncInjectedEvents = [];
var syncInjected = syncInjectedThrowToOuterCatch(syncInjectedEvents);
assert.sameValue(syncInjected.next().value, 'body');
assert.sameValue(syncInjected.throw(new Test262Error('injected')).value, 'undefined');
assert.compareArray(syncInjectedEvents, ['return', 'catch']);

function* syncInnerCatchKeepsIteration(events) {
  for (const value of trackedIterable(1, events)) {
    try {
      yield 'body';
      throw new Test262Error('body');
    } catch (error) {
      events.push('catch');
      yield typeof value;
    }
    break;
  }
}

var syncInnerEvents = [];
assert.compareArray(
  [...syncInnerCatchKeepsIteration(syncInnerEvents)],
  ['body', 'number'],
  'a catch inside the loop retains the active iteration environment'
);
assert.compareArray(syncInnerEvents, ['catch', 'return']);

function* syncHeadFailureToOuterCatch(events) {
  try {
    for (const outer of trackedIterable(1, events)) {
      for (const [inner] of [null]) {
        yield inner;
      }
    }
  } catch (error) {
    events.push('catch');
    yield typeof outer;
  }
}

var syncHeadEvents = [];
assert.compareArray(
  [...syncHeadFailureToOuterCatch(syncHeadEvents)],
  ['undefined'],
  'a nested head failure exits the surrounding loop before its outer catch'
);
assert.compareArray(syncHeadEvents, ['return', 'catch']);

async function* asyncGenerator() {
  for (const value of [5, 6]) {
    yield function () { return value; };
  }
}

async function* asyncThrowToOuterCatch(events) {
  try {
    for (const value of trackedIterable(1, events)) {
      yield 'body';
      throw new Test262Error('body');
    }
  } catch (error) {
    events.push('catch');
    yield typeof value;
  }
}

async function* asyncInjectedThrowToOuterCatch(events) {
  try {
    for (const value of trackedIterable(1, events)) {
      yield 'body';
    }
  } catch (error) {
    events.push('catch');
    yield typeof value;
  }
}

function trackedAsyncIterable(value, events) {
  var done = false;
  return {
    [Symbol.asyncIterator]: function () {
      return {
        next: function () {
          if (done) {
            return Promise.resolve({ value: undefined, done: true });
          }
          done = true;
          return Promise.resolve({ value: value, done: false });
        },
        return: function () {
          events.push('return');
          return { value: undefined, done: true };
        },
      };
    },
  };
}

async function* asyncAwaitThrowToOuterCatch(events) {
  try {
    for await (const value of trackedAsyncIterable(1, events)) {
      yield 'body';
      throw new Test262Error('body');
    }
  } catch (error) {
    events.push('catch');
    yield typeof value;
  }
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

(async function () {
  var asyncClosures = [];
  for await (const closure of asyncGenerator()) {
    asyncClosures.push(closure);
  }

  assert.compareArray(
    asyncClosures.map(function (closure) { return closure(); }),
    [5, 6],
    'async generator iterations have distinct lexical bindings'
  );

  var asyncThrowEvents = [];
  assert.compareArray(
    await collectAsync(asyncThrowToOuterCatch(asyncThrowEvents)),
    ['body', 'undefined'],
    'an outer catch runs outside the exited async-generator iteration environment'
  );
  assert.compareArray(asyncThrowEvents, ['return', 'catch']);

  var asyncInjectedEvents = [];
  var asyncInjected = asyncInjectedThrowToOuterCatch(asyncInjectedEvents);
  assert.sameValue((await asyncInjected.next()).value, 'body');
  assert.sameValue(
    (await asyncInjected.throw(new Test262Error('injected'))).value,
    'undefined'
  );
  assert.compareArray(asyncInjectedEvents, ['return', 'catch']);

  var asyncAwaitEvents = [];
  assert.compareArray(
    await collectAsync(asyncAwaitThrowToOuterCatch(asyncAwaitEvents)),
    ['body', 'undefined'],
    'for-await-of exits its iteration environment before an outer catch'
  );
  assert.compareArray(asyncAwaitEvents, ['return', 'catch']);
})().then($DONE, $DONE);
