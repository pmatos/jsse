/*---
description: >
  A catchless try/finally directly enclosing a finally-less try/catch runs its
  Finally exactly once, regardless of whether the inner catch body's own
  completion (from a rejecting await, a resolving await, a return, or a
  throw raised by an inner finally) is abrupt.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  TryStatement : try Block Finally
  Evaluation of Finally runs exactly once per evaluation of the
  TryStatement, regardless of whether B (the Block's completion) is a throw,
  return, or normal completion.

  Catch : catch ( CatchParameter ) Block
  Step 7: Let B be Completion(Evaluation of Block).
  Step 9: Return ? B.
  An abrupt completion produced by the catch body itself (here, the Throw
  completion an Await resumes with when its promise rejects, per
  sec-await) is returned unchanged -- it is not caught a second time by the
  same Catch.

  TryStatement : try Block Catch
  If B is a throw completion, let C be
  Completion(CatchClauseEvaluation of Catch with argument B.[[Value]]).
  Return ? UpdateEmpty(C, undefined).
  When CatchClauseEvaluation itself returns an abrupt completion, the
  (finally-less) inner TryStatement propagates it unchanged as its own
  completion.
flags: [async]
includes: [compareArray.js]
features: [async-functions]
---*/

function finallyOnceOnRejectedAwaitInNestedCatch() {
  var log = [];
  async function f() {
    try {
      try {
        throw {};
      } catch (e) {
        await Promise.reject(new Error('rejected-in-body'));
      }
    } finally {
      log.push('finally');
    }
  }
  return f().then(
    function () {
      log.push('resolved');
      return log;
    },
    function (e) {
      log.push('rejected:' + e.message);
      return log;
    }
  );
}

function finallyOnceThroughTwoNestedCatchlessTries() {
  var log = [];
  async function f() {
    try {
      try {
        try {
          throw {};
        } catch (e) {
          await Promise.reject(new Error('deep-rejected'));
        }
      } catch (e2) {
        throw e2;
      }
    } finally {
      log.push('finally');
    }
  }
  return f().then(
    function () {
      log.push('resolved');
      return log;
    },
    function (e) {
      log.push('rejected:' + e.message);
      return log;
    }
  );
}

function finallyOnceOnResolvedAwaitInNestedCatch() {
  var log = [];
  async function f() {
    try {
      try {
        throw {};
      } catch (e) {
        await Promise.resolve(1);
        log.push('after-await');
      }
    } finally {
      log.push('finally');
    }
    return 'done';
  }
  return f().then(
    function (v) {
      log.push('resolved:' + v);
      return log;
    },
    function (e) {
      log.push('rejected:' + e);
      return log;
    }
  );
}

function finallyOnceOnThrowFromInnerFinally() {
  var log = [];
  async function f() {
    try {
      try {
        await 1;
      } finally {
        throw new Error('inner-finally-throw');
      }
    } finally {
      log.push('finally');
    }
  }
  return f().then(
    function () {
      log.push('resolved');
      return log;
    },
    function (e) {
      log.push('rejected:' + e.message);
      return log;
    }
  );
}

function finallyOnceOnReturnAfterAwaitInNestedCatch() {
  var log = [];
  async function f() {
    try {
      try {
        throw 1;
      } catch (e) {
        await null;
        return 'r';
      }
    } finally {
      log.push('finally');
    }
  }
  return f().then(
    function (v) {
      log.push('resolved:' + v);
      return log;
    },
    function (e) {
      log.push('rejected:' + e);
      return log;
    }
  );
}

finallyOnceOnRejectedAwaitInNestedCatch()
  .then(function (log) {
    assert.compareArray(
      log,
      ['finally', 'rejected:rejected-in-body'],
      'outer finally runs once and the original rejection propagates'
    );
    return finallyOnceThroughTwoNestedCatchlessTries();
  })
  .then(function (log) {
    assert.compareArray(
      log,
      ['finally', 'rejected:deep-rejected'],
      'outer finally runs once when two stale finally-less contexts sit above it'
    );
    return finallyOnceOnResolvedAwaitInNestedCatch();
  })
  .then(function (log) {
    assert.compareArray(
      log,
      ['after-await', 'finally', 'resolved:done'],
      'a resolving await in the nested catch still reaches the outer finally exactly once'
    );
    return finallyOnceOnThrowFromInnerFinally();
  })
  .then(function (log) {
    assert.compareArray(
      log,
      ['finally', 'rejected:inner-finally-throw'],
      'a throw from the inner try own finally runs the outer finally once'
    );
    return finallyOnceOnReturnAfterAwaitInNestedCatch();
  })
  .then(function (log) {
    assert.compareArray(
      log,
      ['finally', 'resolved:r'],
      'a return after await in the nested catch runs the outer finally once'
    );
  })
  .then($DONE, $DONE);
