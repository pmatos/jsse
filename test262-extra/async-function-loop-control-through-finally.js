/*---
description: >
  Transformed async break and continue completions run intervening finally
  blocks before taking their control-flow targets.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  Evaluation of a try-finally statement evaluates the Finally clause for
  every completion of its try Block. If the Finally clause completes normally,
  the original completion is restored.

  When that completion leaves a for-of body, ForIn/OfBodyEvaluation then
  performs IteratorClose. A finally inside the loop therefore runs before the
  iterator is closed. A finalizer's abrupt completion replaces the pending
  break or continue completion.
flags: [async]
includes: [compareArray.js]
features: [async-functions]
---*/

function trackedIterable(log, name, values) {
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

async function breakThroughAwaitedFinally() {
  var log = [];
  for (const value of trackedIterable(log, 'break', [7])) {
    try {
      await null;
      break;
    } finally {
      log.push('finally before await');
      await null;
      log.push('finally after await ' + value);
    }
  }
  log.push('after break');
  return log;
}

async function rejectedFinallyReplacesBreak() {
  var log = [];
  try {
    for (const value of trackedIterable(log, 'rejected', [1])) {
      try {
        await null;
        break;
      } finally {
        log.push('finally rejects');
        await Promise.reject('finalizer rejection');
      }
    }
  } catch (error) {
    log.push('catch ' + error);
  }
  log.push('after rejection');
  return log;
}

async function breakFromCatchRunsFinally() {
  var log = [];
  for (const value of trackedIterable(log, 'catch break', [1])) {
    try {
      await null;
      throw 'enter catch';
    } catch (error) {
      log.push('catch ' + error);
      break;
    } finally {
      log.push('finally after catch');
    }
  }
  log.push('after catch break');
  return log;
}

async function labeledBreakThroughFinally() {
  var log = [];
  outer: for (const outerValue of trackedIterable(log, 'outer break', [1])) {
    for (const innerValue of trackedIterable(log, 'inner break', [2])) {
      try {
        await null;
        break outer;
      } finally {
        log.push('finally break ' + outerValue + innerValue);
      }
    }
  }
  log.push('after labeled break');
  return log;
}

async function labeledContinueThroughFinally() {
  var log = [];
  outer: for (const outerValue of trackedIterable(log, 'outer continue', [1, 2])) {
    for (const innerValue of trackedIterable(log, 'inner continue ' + outerValue, [3, 4])) {
      try {
        await null;
        continue outer;
      } finally {
        log.push('finally continue ' + outerValue + innerValue);
      }
    }
    log.push('unreachable');
  }
  log.push('after labeled continue');
  return log;
}

async function continueFromNestedTryRunsFinally() {
  var log = [];
  for (const value of [1, 2]) {
    try {
      await null;
      // This nested non-suspending statement executes inline and surfaces its
      // continue completion to the async state-machine driver.
      try {
        continue;
      } finally {
      }
    } finally {
      log.push('finally ' + value);
    }
  }
  log.push('after continue');
  return log;
}

async function labeledContinueFromNestedInlineTry() {
  var log = [];
  outer: for (const outerValue of [1, 2]) {
    for (const innerValue of trackedIterable(log, 'inline ' + outerValue, [3])) {
      await null;
      // This non-suspending try executes inline, so its labelled completion is
      // resolved by the async driver rather than a LoopControl terminator.
      try {
        continue outer;
      } finally {
      }
    }
    log.push('unreachable');
  }
  log.push('after inline labeled continue');
  return log;
}

breakThroughAwaitedFinally()
  .then(function (log) {
    assert.compareArray(
      log,
      ['finally before await', 'finally after await 7', 'close break', 'after break'],
      'an awaited finally retains the loop binding and runs before IteratorClose'
    );
    return rejectedFinallyReplacesBreak();
  })
  .then(function (log) {
    assert.compareArray(
      log,
      ['finally rejects', 'close rejected', 'catch finalizer rejection', 'after rejection'],
      'a rejected await in finally replaces break and still closes before the outer catch'
    );
    return breakFromCatchRunsFinally();
  })
  .then(function (log) {
    assert.compareArray(
      log,
      ['catch enter catch', 'finally after catch', 'close catch break', 'after catch break'],
      'break from catch runs the attached finally before IteratorClose'
    );
    return labeledBreakThroughFinally();
  })
  .then(function (log) {
    assert.compareArray(
      log,
      ['finally break 12', 'close inner break', 'close outer break', 'after labeled break'],
      'labeled break runs finally and closes every exited iterator inner to outer'
    );
    return labeledContinueThroughFinally();
  })
  .then(function (log) {
    assert.compareArray(
      log,
      [
        'finally continue 13',
        'close inner continue 1',
        'finally continue 23',
        'close inner continue 2',
        'after labeled continue'
      ],
      'labeled continue runs finally and closes nested iterators while retaining its target loop'
    );
    return continueFromNestedTryRunsFinally();
  })
  .then(function (log) {
    assert.compareArray(
      log,
      ['finally 1', 'finally 2', 'after continue'],
      'an inline continue completion runs the enclosing finally before reaching the loop head'
    );
    return labeledContinueFromNestedInlineTry();
  })
  .then(function (log) {
    assert.compareArray(
      log,
      ['close inline 1', 'close inline 2', 'after inline labeled continue'],
      'an inline labelled continue targets its outer loop and closes only the inner iterator'
    );
  })
  .then($DONE, $DONE);
