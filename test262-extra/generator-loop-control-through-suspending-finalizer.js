/*---
description: >
  A finally block that suspends (yield or yield*) while running on behalf of
  a break or continue resumes that completion once it finishes; an abrupt
  completion of the finally block, or a return/throw delivered while it is
  suspended, replaces the pending jump.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  Evaluation of a try-finally statement evaluates the Finally clause for
  every completion of its try Block. If the Finally clause completes normally
  the original completion (here a break or continue) is restored; if the
  Finally clause completes abruptly, that completion replaces it.
includes: [compareArray.js]
features: [generators]
---*/

function drain(iterator) {
  var results = [];
  for (var step = iterator.next(); !step.done; step = iterator.next()) {
    results.push(step.value);
  }
  return results;
}

function tracked(log, name, values) {
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
      log.push('close ' + name);
      return { value: undefined, done: true };
    }
  };
}

function* yieldInFinallyOnBreak() {
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
assert.compareArray(drain(yieldInFinallyOnBreak()), ['a', 'f', 'end'], 'yield in finally on break');

function* yieldStarInFinallyOnBreak() {
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
assert.compareArray(
  drain(yieldStarInFinallyOnBreak()),
  ['a', 'f', 'end'],
  'yield* in finally on break'
);

function* yieldInFinallyOnContinue() {
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
assert.compareArray(
  drain(yieldInFinallyOnContinue()),
  ['a0', 'f0', 'a1', 'f1', 'end'],
  'yield in finally on continue'
);

function* nestedSuspendingFinalizers() {
  for (;;) {
    try {
      try {
        yield 'a';
        break;
      } finally {
        yield 'inner';
      }
    } finally {
      yield 'outer';
    }
  }
  yield 'end';
}
assert.compareArray(
  drain(nestedSuspendingFinalizers()),
  ['a', 'inner', 'outer', 'end'],
  'both finalizers suspend'
);

var log = [];
function* suspendingFinalizerThenIteratorClose() {
  for (var x of tracked(log, 'loop', [1, 2])) {
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
assert.compareArray(
  drain(suspendingFinalizerThenIteratorClose()),
  ['a', 'f', 'end'],
  'suspending finalizer before iterator close: values'
);
assert.compareArray(
  log,
  ['finalizer done', 'close loop'],
  'the iterator closes only after the suspended finalizer completes'
);

function* throwReplacesBreak() {
  for (;;) {
    try {
      yield 'a';
      break;
    } finally {
      throw 'from finally';
    }
  }
}
var it = throwReplacesBreak();
it.next();
var thrown;
try {
  it.next();
} catch (e) {
  thrown = e;
}
assert.sameValue(thrown, 'from finally', 'throw from the finalizer replaces the break');

function* returnReplacesBreak() {
  for (;;) {
    try {
      yield 'a';
      break;
    } finally {
      return 'from finally';
    }
  }
  yield 'WRONG after loop';
}
it = returnReplacesBreak();
it.next();
var result = it.next();
assert.sameValue(result.value, 'from finally', 'return value replaces the break');
assert.sameValue(result.done, true, 'return completes the generator');

function* breakReplacesContinue() {
  for (var i = 0; i < 3; i++) {
    try {
      yield 'a' + i;
      continue;
    } finally {
      break;
    }
  }
  yield 'end';
}
assert.compareArray(drain(breakReplacesContinue()), ['a0', 'end'], 'break replaces continue');

function* continueReplacesBreak() {
  for (var i = 0; i < 2; i++) {
    try {
      yield 'a' + i;
      break;
    } finally {
      continue;
    }
  }
  yield 'end';
}
assert.compareArray(
  drain(continueReplacesBreak()),
  ['a0', 'a1', 'end'],
  'continue replaces break'
);

function* labeledBreakReplacesBreak() {
  outer: for (;;) {
    for (;;) {
      try {
        yield 'a';
        break;
      } finally {
        break outer;
      }
    }
    yield 'WRONG after inner loop';
  }
  yield 'end';
}
assert.compareArray(
  drain(labeledBreakReplacesBreak()),
  ['a', 'end'],
  'a labelled break in the finalizer replaces the inner break'
);

log = [];
function* innerLoopInFinalizerKeepsPendingBreak() {
  for (;;) {
    try {
      yield 'a';
      break;
    } finally {
      for (var j = 0; j < 2; j++) {
        try {
          yield 'f' + j;
          break;
        } finally {
          log.push('inner' + j);
        }
      }
      yield 'tail';
    }
    yield 'WRONG after try';
  }
  yield log.join();
}
assert.compareArray(
  drain(innerLoopInFinalizerKeepsPendingBreak()),
  ['a', 'f0', 'tail', 'inner0'],
  "a break inside the finalizer's own loop leaves the pending break intact"
);

function* breakInFinalizerLoopKeepsPendingThrow() {
  try {
    throw 'pending';
  } finally {
    for (var i = 0; i < 2; i++) {
      yield i;
      break;
    }
    yield 'after';
  }
}
it = breakInFinalizerLoopKeepsPendingThrow();
assert.sameValue(it.next().value, 0, 'pending throw: first yield');
assert.sameValue(it.next().value, 'after', 'pending throw: after the inner loop');
thrown = undefined;
try {
  it.next();
} catch (e) {
  thrown = e;
}
assert.sameValue(thrown, 'pending', 'the pending throw survives a break inside the finalizer');

function* breakReplacesPendingThrow() {
  for (;;) {
    try {
      throw 'pending';
    } finally {
      yield 'f';
      break;
    }
  }
  yield 'end';
}
assert.compareArray(
  drain(breakReplacesPendingThrow()),
  ['f', 'end'],
  'a break leaving the finalizer replaces the pending throw'
);

function* returnDuringSuspendedFinalizer() {
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
log = [];
it = returnDuringSuspendedFinalizer();
it.next();
assert.sameValue(it.next().value, 'f', 'return(): parked in finalizer');
result = it.return('r');
assert.sameValue(result.value, 'r', 'return() replaces the pending break');
assert.sameValue(result.done, true, 'return() completes the generator');
assert.compareArray(log, [], 'the finalizer body does not resume after return()');

function* throwDuringSuspendedFinalizer() {
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
log = [];
it = throwDuringSuspendedFinalizer();
it.next();
assert.sameValue(it.next().value, 'f', 'throw(): parked in finalizer');
assert.sameValue(it.throw('E').value, 'caught E', 'throw() replaces the pending break');
assert.compareArray(log, [], 'the finalizer body does not resume after throw()');

function* caughtThrowInsideFinalizerKeepsBreak() {
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
it = caughtThrowInsideFinalizerKeepsBreak();
it.next();
assert.sameValue(it.next().value, 'f', 'inner catch: parked in finalizer');
assert.sameValue(
  it.throw('E').value,
  'caught E',
  'a throw caught inside the finalizer leaves the pending break intact'
);
assert.sameValue(it.next().done, true, 'inner catch: done');
