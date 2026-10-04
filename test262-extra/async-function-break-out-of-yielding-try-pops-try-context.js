/*---
description: >
  A break or continue that leaves a suspended try statement discards that try
  context, so a later throw is not caught by its stale catch clause and its
  finally clause does not run a second time.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  Evaluation of a try statement completes once its Block (or Catch) completes.
  A break or continue completion that leaves it hands control to the loop or
  labelled statement outside; the statement is no longer active, so an
  exception thrown afterwards cannot be caught by its catch clause, and its
  Finally clause has already been evaluated exactly once.
flags: [async]
includes: [compareArray.js]
features: [async-functions]
---*/

async function breakDoesNotLeaveLiveCatch() {
  var caught = 0;
  var iterations = 0;
  for (;;) {
    if (++iterations > 3) {
      caught = 'WRONG catch re-entered loop';
      break;
    }
    try {
      await 1;
      break;
    } catch (e) {
      caught++;
    }
  }
  throw 'after loop';
}

async function continueDoesNotLeaveLiveCatch() {
  var caught = 0;
  var iterations = 0;
  for (var i = 0; i < 2; i++) {
    if (++iterations > 5) {
      caught = 'WRONG catch re-entered loop';
      break;
    }
    try {
      await 1;
      continue;
    } catch (e) {
      caught++;
    }
  }
  throw 'after loop';
}

async function finallyRunsOnceThroughCatchOnlyInner() {
  var log = [];
  for (var i = 0; i < 1; i++) {
    try {
      try {
        await null;
        break;
      } catch (e) {
        log.push('WRONG inner catch');
      }
    } finally {
      log.push('finally');
    }
  }
  return log;
}

async function labeledBreakPopsAllTryContexts() {
  var log = [];
  outer: for (var i = 0; i < 1; i++) {
    for (var j = 0; j < 1; j++) {
      try {
        await null;
        break outer;
      } catch (e) {
        log.push('WRONG catch');
      }
    }
  }
  try {
    throw 'later';
  } catch (e) {
    log.push('caught ' + e);
  }
  return log;
}

Promise.all([
  breakDoesNotLeaveLiveCatch().then(
    function () { throw new Test262Error('expected rejection'); },
    function (e) { assert.sameValue(e, 'after loop', 'break: throw after the loop propagates'); }
  ),
  continueDoesNotLeaveLiveCatch().then(
    function () { throw new Test262Error('expected rejection'); },
    function (e) { assert.sameValue(e, 'after loop', 'continue: throw after the loop propagates'); }
  ),
  finallyRunsOnceThroughCatchOnlyInner().then(function (log) {
    assert.compareArray(log, ['finally'], 'finally runs exactly once');
  }),
  labeledBreakPopsAllTryContexts().then(function (log) {
    assert.compareArray(log, ['caught later'], 'labeled break');
  })
]).then(function () { $DONE(); }, $DONE);
