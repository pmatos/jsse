/*---
description: >
  Async for-of unwinding carries abrupt completions through nested iteration
  disposal and iterator closing.
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
info: |
  When a for-of body completes abruptly, ForIn/OfBodyEvaluation first disposes
  the current iteration environment and then performs IteratorClose. Each
  enclosing loop receives that resulting completion as its own body result.

  DisposeResources combines a new disposal error with an existing throw in a
  SuppressedError whose error is the new error and whose suppressed value is
  the prior abrupt completion.
flags: [async]
features: [async-functions, explicit-resource-management]
---*/

function throwingResource(name) {
  return {
    [Symbol.dispose]: function () {
      throw name;
    }
  };
}

async function nestedDisposalErrors() {
  for (using outer of [throwingResource('outer')]) {
    for (using inner of [throwingResource('inner')]) {
      await null;
      return 'unreachable';
    }
  }
}

var throwingCloseIterator = {
  done: false,
  [Symbol.iterator]: function () {
    return this;
  },
  next: function () {
    if (this.done) {
      return { value: undefined, done: true };
    }
    this.done = true;
    return { value: 1, done: false };
  },
  return: function () {
    throw 'close';
  }
};

async function closeFailureEscapesExitedCatch() {
  for (const value of throwingCloseIterator) {
    try {
      await null;
      return value;
    } catch (error) {
      return 'incorrectly caught ' + error;
    }
  }
}

var retainedCloseCount = 0;
var retainedIterator = {
  done: false,
  [Symbol.iterator]: function () {
    return this;
  },
  next: function () {
    if (this.done) {
      return { value: undefined, done: true };
    }
    this.done = true;
    return { value: 2, done: false };
  },
  return: function () {
    retainedCloseCount += 1;
    return { value: undefined, done: true };
  }
};

async function finallyThrowOverridesReturn() {
  for (const value of retainedIterator) {
    try {
      await null;
      return value;
    } finally {
      throw 'finally';
    }
  }
}

var rejectedAwaitCloseCount = 0;
var rejectedAwaitIterator = {
  done: false,
  [Symbol.iterator]: function () {
    return this;
  },
  next: function () {
    if (this.done) {
      return { value: undefined, done: true };
    }
    this.done = true;
    return { value: 3, done: false };
  },
  return: function () {
    rejectedAwaitCloseCount += 1;
    return { value: undefined, done: true };
  }
};

async function rejectFromFinally() {
  throw 'awaited finally';
}

async function finallyRejectedAwaitOverridesReturn() {
  for (const value of rejectedAwaitIterator) {
    try {
      await null;
      return value;
    } finally {
      await rejectFromFinally();
    }
  }
}

var outerFinallyOrder = [];
var outerFinallyIterator = {
  done: false,
  [Symbol.iterator]: function () {
    return this;
  },
  next: function () {
    if (this.done) {
      return { value: undefined, done: true };
    }
    this.done = true;
    return { value: 4, done: false };
  },
  return: function () {
    outerFinallyOrder.push('close');
    return { value: undefined, done: true };
  }
};

async function throwingFinallyInsideOuterFinally() {
  try {
    for (const value of outerFinallyIterator) {
      try {
        await null;
        return value;
      } finally {
        throw 'nested finally';
      }
    }
  } finally {
    outerFinallyOrder.push('outer finally');
  }
}

nestedDisposalErrors()
  .then(
    function () {
      throw new Test262Error('nested disposal errors must reject');
    },
    function (error) {
      assert(error instanceof SuppressedError, 'outer disposal creates a SuppressedError');
      assert.sameValue(error.error, 'outer', 'outer disposal error is the primary error');
      assert.sameValue(error.suppressed, 'inner', 'inner disposal error is suppressed');
    }
  )
  .then(function () {
    return closeFailureEscapesExitedCatch();
  })
  .then(
    function (value) {
      throw new Test262Error('iterator close failure resolved with ' + value);
    },
    function (error) {
      assert.sameValue(error, 'close', 'an exited catch cannot handle IteratorClose failure');
    }
  )
  .then(function () {
    return finallyThrowOverridesReturn();
  })
  .then(
    function (value) {
      throw new Test262Error('throwing finally resolved with ' + value);
    },
    function (error) {
      assert.sameValue(error, 'finally', 'finally throw replaces the pending return');
      assert.sameValue(retainedCloseCount, 1, 'the loop retained for finally is still closed');
    }
  )
  .then(function () {
    return finallyRejectedAwaitOverridesReturn();
  })
  .then(
    function (value) {
      throw new Test262Error('rejected await in finally resolved with ' + value);
    },
    function (error) {
      assert.sameValue(error, 'awaited finally', 'rejected await replaces the pending return');
      assert.sameValue(rejectedAwaitCloseCount, 1, 'retained loop closes after rejected await');
    }
  )
  .then(function () {
    return throwingFinallyInsideOuterFinally();
  })
  .then(
    function (value) {
      throw new Test262Error('nested throwing finally resolved with ' + value);
    },
    function (error) {
      assert.sameValue(error, 'nested finally', 'inner finally throw remains the rejection');
      assert.sameValue(
        outerFinallyOrder.join(','),
        'close,outer finally',
        'retained loop closes before a finally outside the loop'
      );
    }
  )
  .then($DONE, $DONE);
