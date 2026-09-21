/*---
description: >
  A break or continue that leaves one or more for-await-of loops in an async
  generator closes each iterator, after any finally block nested inside the
  loop body has run.
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
info: |
  ForIn/OfBodyEvaluation: if LoopContinues(result, labelSet) is false, the
  iterator is closed with AsyncIteratorClose. A labelled break targeting a
  statement outside the loop is not a continuing completion, so it closes the
  loop's iterator. A finally block inside the loop body completes before the
  body's completion reaches the loop.
flags: [async]
includes: [compareArray.js, asyncHelpers.js]
features: [async-iteration]
---*/

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

async function drain(iterator) {
  var results = [];
  for (var step = await iterator.next(); !step.done; step = await iterator.next()) {
    results.push(step.value);
  }
  return results;
}

var log = [];
async function* labeledBreakClosesIterator() {
  outer: while (true) {
    for await (var x of tracked(log, 'inner', [1, 2])) {
      yield x;
      break outer;
    }
  }
  yield 'after';
}

async function* labeledContinueClosesInnerIterator() {
  outer: for (var i = 0; i < 2; i++) {
    for await (var x of tracked(log, 'inner' + i, [1, 2])) {
      yield i + ':' + x;
      continue outer;
    }
  }
  yield 'after';
}

async function* finallyRunsBeforeIteratorClose() {
  for await (var x of tracked(log, 'loop', [1, 2])) {
    try {
      yield x;
      break;
    } finally {
      log.push('finally ' + x);
    }
  }
  yield 'after';
}

async function* finallyAndCloseAcrossTwoLoops() {
  outer: for await (var a of tracked(log, 'outer', [1, 2])) {
    for await (var b of tracked(log, 'inner', [3, 4])) {
      try {
        yield a + ':' + b;
        break outer;
      } finally {
        log.push('finally ' + a + b);
      }
    }
  }
  yield 'after';
}

async function* enclosingFinallyRunsAfterInnerClose() {
  try {
    for await (var x of tracked(log, 'loop', [1, 2])) {
      try {
        yield x;
        break;
      } finally {
        log.push('inner finally');
      }
    }
  } finally {
    log.push('outer finally');
  }
  yield 'after';
}

async function main() {
  log = [];
  assert.compareArray(await drain(labeledBreakClosesIterator()), [1, 'after'], 'labelled break: values');
  assert.compareArray(log, ['close inner'], 'labelled break closes the iterator exactly once');

  log = [];
  assert.compareArray(
    await drain(labeledContinueClosesInnerIterator()),
    ['0:1', '1:1', 'after'],
    'labelled continue: values'
  );
  assert.compareArray(
    log,
    ['close inner0', 'close inner1'],
    'labelled continue closes the inner iterators'
  );

  log = [];
  assert.compareArray(
    await drain(finallyRunsBeforeIteratorClose()),
    [1, 'after'],
    'finally then close: values'
  );
  assert.compareArray(
    log,
    ['finally 1', 'close loop'],
    'the finally in the loop body runs before AsyncIteratorClose'
  );

  log = [];
  assert.compareArray(
    await drain(finallyAndCloseAcrossTwoLoops()),
    ['1:3', 'after'],
    'two loops: values'
  );
  assert.compareArray(
    log,
    ['finally 13', 'close inner', 'close outer'],
    'the finalizer runs, then the inner and outer iterators close, innermost first'
  );

  log = [];
  assert.compareArray(
    await drain(enclosingFinallyRunsAfterInnerClose()),
    [1, 'after'],
    'enclosing finally: values'
  );
  assert.compareArray(
    log,
    ['inner finally', 'close loop', 'outer finally'],
    'a finally enclosing the loop runs after the loop closed its iterator'
  );
}

asyncTest(main);
