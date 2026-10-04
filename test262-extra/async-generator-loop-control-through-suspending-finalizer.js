/*---
description: >
  A finally block that suspends (await, yield, or yield*) while running on
  behalf of a break or continue in an async generator resumes that completion
  once it finishes; an abrupt completion of the finally block, or a
  return/throw delivered while it is suspended, replaces the pending jump.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  Evaluation of a try-finally statement evaluates the Finally clause for
  every completion of its try Block. If the Finally clause completes normally
  the original completion (here a break or continue) is restored; if the
  Finally clause completes abruptly, that completion replaces it.
flags: [async]
includes: [compareArray.js, asyncHelpers.js]
features: [async-iteration]
---*/

async function drain(iterator) {
  var results = [];
  for (var step = await iterator.next(); !step.done; step = await iterator.next()) {
    results.push(step.value);
  }
  return results;
}

async function outcome(promise) {
  try {
    return { fulfilled: await promise };
  } catch (e) {
    return { rejected: e };
  }
}

function tracked(log, name, values) {
  var index = 0;
  return {
    [Symbol.asyncIterator]: function () {
      return this;
    },
    next: function () {
      return Promise.resolve(
        index < values.length
          ? { value: values[index++], done: false }
          : { value: undefined, done: true }
      );
    },
    return: function () {
      log.push('close ' + name);
      return Promise.resolve({ value: undefined, done: true });
    }
  };
}

var log = [];

async function* awaitInFinallyOnBreak() {
  var log = [];
  for (;;) {
    try {
      yield 'a';
      break;
    } finally {
      log.push('before await');
      await null;
      log.push('after await');
    }
  }
  yield log.join();
}

async function* yieldInFinallyOnBreak() {
  for (;;) {
    try {
      yield 'a';
      break;
    } finally {
      yield 'f';
    }
  }
  yield 'end';
}

async function* yieldStarInFinallyOnBreak() {
  for (;;) {
    try {
      yield 'a';
      break;
    } finally {
      yield* ['f'];
    }
  }
  yield 'end';
}

async function* yieldInFinallyOnContinue() {
  for (var i = 0; i < 2; i++) {
    try {
      yield 'a' + i;
      continue;
    } finally {
      yield 'f' + i;
    }
  }
  yield 'end';
}

async function* nestedSuspendingFinalizers() {
  for (;;) {
    try {
      try {
        yield 'a';
        break;
      } finally {
        await null;
        yield 'inner';
      }
    } finally {
      yield 'outer';
    }
  }
  yield 'end';
}

async function* suspendingFinalizerThenIteratorClose() {
  for await (var x of tracked(log, 'loop', [1, 2])) {
    try {
      yield 'a';
      break;
    } finally {
      yield 'f';
      log.push('finalizer done');
    }
  }
  yield 'end';
}

async function* throwReplacesBreak() {
  for (;;) {
    try {
      yield 'a';
      break;
    } finally {
      await null;
      throw 'from finally';
    }
  }
}

async function* returnReplacesBreak() {
  for (;;) {
    try {
      yield 'a';
      break;
    } finally {
      await null;
      return 'from finally';
    }
  }
  yield 'WRONG after loop';
}

async function* breakReplacesContinue() {
  for (var i = 0; i < 3; i++) {
    try {
      yield 'a' + i;
      continue;
    } finally {
      await null;
      break;
    }
  }
  yield 'end';
}

async function* continueReplacesBreak() {
  for (var i = 0; i < 2; i++) {
    try {
      yield 'a' + i;
      break;
    } finally {
      await null;
      continue;
    }
  }
  yield 'end';
}

async function* returnDuringSuspendedFinalizer() {
  for (;;) {
    try {
      yield 'a';
      break;
    } finally {
      yield 'f';
      log.push('WRONG finalizer resumed');
    }
  }
  yield 'WRONG after loop';
}

async function* throwDuringSuspendedFinalizer() {
  try {
    for (;;) {
      try {
        yield 'a';
        break;
      } finally {
        yield 'f';
        log.push('WRONG finalizer resumed');
      }
    }
    yield 'WRONG after loop';
  } catch (e) {
    yield 'caught ' + e;
  }
}

async function* caughtThrowInsideFinalizerKeepsBreak() {
  var log = [];
  for (;;) {
    try {
      yield 'a';
      break;
    } finally {
      try {
        yield 'f';
      } catch (e) {
        log.push('caught ' + e);
      }
    }
    yield 'WRONG after try';
  }
  yield log.join();
}

async function main() {
  assert.compareArray(
    await drain(awaitInFinallyOnBreak()),
    ['a', 'before await,after await'],
    'await in finally on break'
  );
  assert.compareArray(await drain(yieldInFinallyOnBreak()), ['a', 'f', 'end'], 'yield in finally');
  assert.compareArray(
    await drain(yieldStarInFinallyOnBreak()),
    ['a', 'f', 'end'],
    'yield* in finally'
  );
  assert.compareArray(
    await drain(yieldInFinallyOnContinue()),
    ['a0', 'f0', 'a1', 'f1', 'end'],
    'yield in finally on continue'
  );
  assert.compareArray(
    await drain(nestedSuspendingFinalizers()),
    ['a', 'inner', 'outer', 'end'],
    'both finalizers suspend'
  );

  log = [];
  assert.compareArray(
    await drain(suspendingFinalizerThenIteratorClose()),
    ['a', 'f', 'end'],
    'suspending finalizer before iterator close: values'
  );
  assert.compareArray(
    log,
    ['finalizer done', 'close loop'],
    'the iterator closes only after the suspended finalizer completes'
  );

  var it = throwReplacesBreak();
  await it.next();
  assert.sameValue(
    (await outcome(it.next())).rejected,
    'from finally',
    'throw from the finalizer replaces the break'
  );

  it = returnReplacesBreak();
  await it.next();
  var result = await it.next();
  assert.sameValue(result.value, 'from finally', 'return value replaces the break');
  assert.sameValue(result.done, true, 'return completes the generator');

  assert.compareArray(
    await drain(breakReplacesContinue()),
    ['a0', 'end'],
    'break replaces continue'
  );
  assert.compareArray(
    await drain(continueReplacesBreak()),
    ['a0', 'a1', 'end'],
    'continue replaces break'
  );

  log = [];
  it = returnDuringSuspendedFinalizer();
  await it.next();
  assert.sameValue((await it.next()).value, 'f', 'return(): parked in finalizer');
  result = await it.return('r');
  assert.sameValue(result.value, 'r', 'return() replaces the pending break');
  assert.sameValue(result.done, true, 'return() completes the generator');
  assert.compareArray(log, [], 'the finalizer body does not resume after return()');

  log = [];
  it = throwDuringSuspendedFinalizer();
  await it.next();
  assert.sameValue((await it.next()).value, 'f', 'throw(): parked in finalizer');
  assert.sameValue(
    (await it.throw('E')).value,
    'caught E',
    'throw() replaces the pending break'
  );
  assert.compareArray(log, [], 'the finalizer body does not resume after throw()');

  it = caughtThrowInsideFinalizerKeepsBreak();
  await it.next();
  assert.sameValue((await it.next()).value, 'f', 'inner catch: parked in finalizer');
  assert.sameValue(
    (await it.throw('E')).value,
    'caught E',
    'a throw caught inside the finalizer leaves the pending break intact'
  );
  assert.sameValue((await it.next()).done, true, 'inner catch: done');
}

asyncTest(main);
