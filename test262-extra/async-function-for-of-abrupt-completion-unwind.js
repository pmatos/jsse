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

var caughtCloseOrder = [];

function trackedCloseIterator(name, error) {
  return {
    done: false,
    [Symbol.iterator]: function () {
      return this;
    },
    next: function () {
      if (this.done) {
        return { value: undefined, done: true };
      }
      this.done = true;
      return { value: name, done: false };
    },
    return: function () {
      caughtCloseOrder.push('close ' + name);
      if (error !== undefined) {
        throw error;
      }
      return { value: undefined, done: true };
    }
  };
}

var caughtCloseOuter = trackedCloseIterator('outer');
var caughtCloseInner = trackedCloseIterator('inner', 'inner close');

async function innerCloseFailureReachesCatchBeforeOuterClose() {
  for (const outerValue of caughtCloseOuter) {
    try {
      for (const innerValue of caughtCloseInner) {
        await null;
        return outerValue + innerValue;
      }
      caughtCloseOrder.push('inner exhausted');
    } catch (error) {
      caughtCloseOrder.push('catch ' + error);
      return 'caught';
    }
  }
}

function replacementCloseIterator(log, name, error) {
  return {
    done: false,
    [Symbol.iterator]: function () {
      return this;
    },
    next: function () {
      if (this.done) {
        return { value: undefined, done: true };
      }
      this.done = true;
      return { value: name, done: false };
    },
    return: function () {
      log.push('close ' + name);
      if (error !== undefined) {
        throw error;
      }
      return { value: undefined, done: true };
    }
  };
}

var throwingCatchOrder = [];
var throwingCatchOuter = replacementCloseIterator(throwingCatchOrder, 'outer');
var throwingCatchInner = replacementCloseIterator(
  throwingCatchOrder,
  'inner',
  'inner close before catch throw'
);

async function catchThrowContinuesRetainedUnwind() {
  for (const outerValue of throwingCatchOuter) {
    try {
      for (const innerValue of throwingCatchInner) {
        await null;
        return outerValue + innerValue;
      }
      throwingCatchOrder.push('inner exhausted');
    } catch (error) {
      throwingCatchOrder.push('catch ' + error);
      throw 'catch replacement';
    }
  }
}

var rejectingCatchOrder = [];
var rejectingCatchOuter = replacementCloseIterator(rejectingCatchOrder, 'outer');
var rejectingCatchInner = replacementCloseIterator(
  rejectingCatchOrder,
  'inner',
  'inner close before catch rejection'
);

async function catchRejectedAwaitContinuesRetainedUnwind() {
  for (const outerValue of rejectingCatchOuter) {
    try {
      for (const innerValue of rejectingCatchInner) {
        await null;
        return outerValue + innerValue;
      }
      rejectingCatchOrder.push('inner exhausted');
    } catch (error) {
      rejectingCatchOrder.push('catch ' + error);
      await Promise.reject('catch rejection');
    }
  }
}

var normalCatchOrder = [];
var normalCatchOuter = {
  calls: 0,
  [Symbol.iterator]: function () {
    return this;
  },
  next: function () {
    this.calls += 1;
    if (this.calls === 1) {
      return { value: 'outer', done: false };
    }
    normalCatchOrder.push('outer next');
    throw 'outer next failure';
  },
  return: function () {
    normalCatchOrder.push('incorrect outer close');
    return { value: undefined, done: true };
  }
};
var normalCatchInner = replacementCloseIterator(
  normalCatchOrder,
  'inner',
  'inner close before normal catch'
);

async function normalCatchClearsRetainedUnwind() {
  for (const outerValue of normalCatchOuter) {
    try {
      for (const innerValue of normalCatchInner) {
        await null;
        return outerValue + innerValue;
      }
      normalCatchOrder.push('inner exhausted');
    } catch (error) {
      normalCatchOrder.push('catch ' + error);
      await null;
    }
    normalCatchOrder.push('after catch');
  }
}

var finallyCloseOrder = [];

function finallyCloseIterator(name, error) {
  return {
    done: false,
    [Symbol.iterator]: function () {
      return this;
    },
    next: function () {
      if (this.done) {
        return { value: undefined, done: true };
      }
      this.done = true;
      return { value: name, done: false };
    },
    return: function () {
      finallyCloseOrder.push('close ' + name);
      if (error !== undefined) {
        throw error;
      }
      return { value: undefined, done: true };
    }
  };
}

var finallyCloseOuter = finallyCloseIterator('outer');
var finallyCloseInner = finallyCloseIterator('inner', 'inner close through finally');

async function innerCloseFailureRunsFinallyBeforeOuterClose() {
  for (const outerValue of finallyCloseOuter) {
    try {
      for (const innerValue of finallyCloseInner) {
        await null;
        return outerValue + innerValue;
      }
      finallyCloseOrder.push('inner exhausted');
    } finally {
      await null;
      finallyCloseOrder.push('finally');
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
    return innerCloseFailureReachesCatchBeforeOuterClose();
  })
  .then(function (value) {
    assert.sameValue(value, 'caught', 'the intervening catch handles the inner close failure');
    assert.sameValue(
      caughtCloseOrder.join(','),
      'close inner,catch inner close,close outer',
      'the close failure routes through the catch before outer loop unwinding'
    );
  })
  .then(function () {
    return catchThrowContinuesRetainedUnwind();
  })
  .then(
    function (value) {
      throw new Test262Error('throwing catch resolved with ' + value);
    },
    function (error) {
      assert.sameValue(error, 'catch replacement', 'the catch throw replaces the close failure');
      assert.sameValue(
        throwingCatchOrder.join(','),
        'close inner,catch inner close before catch throw,close outer',
        'the catch throw resumes retained outer loop unwinding'
      );
    }
  )
  .then(function () {
    return catchRejectedAwaitContinuesRetainedUnwind();
  })
  .then(
    function (value) {
      throw new Test262Error('rejected await in catch resolved with ' + value);
    },
    function (error) {
      assert.sameValue(error, 'catch rejection', 'the rejected await replaces the close failure');
      assert.sameValue(
        rejectingCatchOrder.join(','),
        'close inner,catch inner close before catch rejection,close outer',
        'the retained unwind survives suspension in the catch'
      );
    }
  )
  .then(function () {
    return normalCatchClearsRetainedUnwind();
  })
  .then(
    function (value) {
      throw new Test262Error('outer next failure resolved with ' + value);
    },
    function (error) {
      assert.sameValue(error, 'outer next failure', 'the later IteratorNext error rejects');
      assert.sameValue(
        normalCatchOrder.join(','),
        'close inner,catch inner close before normal catch,after catch,outer next',
        'normal catch completion clears retained unwind before IteratorNext'
      );
    }
  )
  .then(function () {
    return innerCloseFailureRunsFinallyBeforeOuterClose();
  })
  .then(
    function (value) {
      throw new Test262Error('inner close failure through finally resolved with ' + value);
    },
    function (error) {
      assert.sameValue(error, 'inner close through finally', 'the close failure remains abrupt');
      assert.sameValue(
        finallyCloseOrder.join(','),
        'close inner,finally,close outer',
        'an intervening finally runs before unwinding the outer loop'
      );
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
