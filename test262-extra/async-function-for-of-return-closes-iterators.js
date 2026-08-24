/*---
description: >
  A return that leaves one or more async for-of loops closes every active
  iterator, inner to outer, interleaved with the finally blocks it unwinds
  through, while retaining a lexical iteration binding until an awaited
  finally completes.
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
info: |
  ForIn/OfBodyEvaluation step 7.m: if the loop body produces an abrupt
  completion, return ? IteratorClose(iteratorRecord, status). A return
  completion therefore closes the iterator of every for-of it leaves, from the
  innermost outwards.

  A try statement lexically inside the loop completes abruptly before the
  for-of statement does, so its finally runs before that loop's IteratorClose;
  a try statement enclosing the loop runs its finally afterwards.

  For a lexical loop head, ForIn/OfBodyEvaluation evaluates the statement with
  the iteration environment active and restores oldEnv only after that
  evaluation completes. Await in a finally that is handling a pending return
  must therefore resume with the iteration binding still available.
flags: [async]
includes: [compareArray.js]
features: [async-functions]
---*/

var log = [];

function trackedIterable(name, values) {
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
      log.push('close:' + name);
      return { done: true };
    }
  };
}

async function identity(value) {
  return value;
}

// The return expression itself suspends, so the state machine — not the
// ordinary statement executor — is responsible for the iterator close.
async function returnAcrossNestedLoops() {
  for (const outer of trackedIterable('outer', [1, 2])) {
    for (const inner of trackedIterable('inner', [1, 2])) {
      return await identity(outer + inner);
    }
  }
  return -1;
}

async function returnAfterAwait() {
  for (const value of trackedIterable('afterAwait', [1, 2])) {
    await null;
    return value;
  }
  return -1;
}

// A finally lexically outside the loop runs after the loop's IteratorClose.
async function returnThroughOuterFinally() {
  try {
    for (const value of trackedIterable('outerFinally', [1, 2])) {
      await null;
      return value;
    }
  } finally {
    log.push('finally');
  }
  return -1;
}

// A finally lexically inside the loop runs before it, because the try
// statement completes abruptly before the for-of statement does.
async function returnThroughInnerFinally() {
  for (const value of trackedIterable('innerFinally', [1, 2])) {
    try {
      await null;
      return value;
    } finally {
      log.push('finally');
    }
  }
  return -1;
}

// Keep the active iteration environment while a return is pending through an
// awaited finally. The conditional keeps the return in the transformed body
// rather than making it the state's direct terminator.
async function returnRetainsBindingThroughAwaitedFinally(shouldReturn) {
  for (const value of trackedIterable('awaitedFinally', [7, 8])) {
    try {
      log.push('before:' + value);
      if (shouldReturn) return value;
    } finally {
      await null;
      log.push('after:' + value);
    }
  }
  return -1;
}

returnAcrossNestedLoops()
  .then(function (value) {
    assert.sameValue(value, 2, 'nested return resolves with the awaited value');
    assert.compareArray(
      log,
      ['close:inner', 'close:outer'],
      'nested loops close inner to outer'
    );
    log.length = 0;
    return returnAfterAwait();
  })
  .then(function (value) {
    assert.sameValue(value, 1, 'return after await resolves with the loop value');
    assert.compareArray(log, ['close:afterAwait'], 'a single loop closes its iterator');
    log.length = 0;
    return returnThroughOuterFinally();
  })
  .then(function (value) {
    assert.sameValue(value, 1, 'return through an outer finally resolves with the loop value');
    assert.compareArray(
      log,
      ['close:outerFinally', 'finally'],
      'the iterator closes before a finally outside the loop runs'
    );
    log.length = 0;
    return returnThroughInnerFinally();
  })
  .then(function (value) {
    assert.sameValue(value, 1, 'return through an inner finally resolves with the loop value');
    assert.compareArray(
      log,
      ['finally', 'close:innerFinally'],
      'a finally inside the loop runs before the iterator closes'
    );
    log.length = 0;
    return returnRetainsBindingThroughAwaitedFinally(true);
  })
  .then(function (value) {
    assert.sameValue(value, 7, 'the pending return keeps its original value');
    assert.compareArray(
      log,
      ['before:7', 'after:7', 'close:awaitedFinally'],
      'the loop binding remains available after await and before IteratorClose'
    );
  })
  .then($DONE, $DONE);
