/*---
description: >
  A try/catch with no finally clause must not leave its runtime bookkeeping
  behind once it completes: a later break/continue's finalizer routing counts
  try/catch nesting depth, and a leaked finally-less try context shifts every
  depth computed afterwards, misrouting or re-running unrelated finally
  blocks.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  Evaluation of a try-finally statement evaluates the Finally clause exactly
  once for every completion of its try Block, and a try/catch with no
  Finally has no runtime state left to track once its Catch (or Block, if
  uncaught) completes.
features: [generators]
includes: [compareArray.js]
---*/

function drain(iterator) {
  var results = [];
  for (var step = iterator.next(); !step.done; step = iterator.next()) {
    results.push(step.value);
  }
  return results;
}

// A finally-less try/catch completing before a break inside a later
// try/finally must not cause that finally to run twice.
var log = [];
function* finallyLessTryBeforeBreakingFinally() {
  try {
    yield 0;
  } catch (e) {}
  try {
    for (var i = 0; i < 3; i++) {
      yield i;
      if (i === 1) break;
    }
    log.push('after loop');
  } finally {
    log.push('outer finally');
  }
  log.push('end');
}
drain(finallyLessTryBeforeBreakingFinally());
assert.compareArray(
  log,
  ['after loop', 'outer finally', 'end'],
  'the outer finally runs exactly once, not twice'
);

// A finally-less try/catch nested inside a suspending finalizer must not
// desync the loop's own finally so that it runs forever instead of once.
function* finallyLessTryInsideBreakingFinally() {
  for (;;) {
    try {
      yield 1;
      break;
    } finally {
      try {
        yield 'f';
      } catch (e) {}
    }
  }
  yield 'end';
}
assert.compareArray(
  drain(finallyLessTryInsideBreakingFinally()),
  [1, 'f', 'end'],
  'the loop terminates after the finally runs once, instead of looping forever'
);

// The same shape with continue, and multiple loop iterations, to exercise
// repeated push/pop of the finally-less context across iterations.
log = [];
function* finallyLessTryBeforeContinuingFinally() {
  for (var i = 0; i < 2; i++) {
    try {
      yield 'a' + i;
    } catch (e) {}
    try {
      yield 'b' + i;
      continue;
    } finally {
      log.push('fin' + i);
    }
    log.push('WRONG unreachable' + i);
  }
  yield 'end';
}
assert.compareArray(
  drain(finallyLessTryBeforeContinuingFinally()),
  ['a0', 'b0', 'a1', 'b1', 'end'],
  'continue values with an interleaved finally-less try'
);
assert.compareArray(log, ['fin0', 'fin1'], 'each iteration finally runs exactly once');
