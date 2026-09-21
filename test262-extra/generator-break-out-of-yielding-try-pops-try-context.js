/*---
description: >
  A break or continue that leaves a try statement containing a yield discards
  that try context, so a later throw is not caught by its stale catch clause.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  Evaluation of a try statement completes once its Block completes. A break or
  continue completion that leaves it hands control to the enclosing loop; the
  try statement is no longer active, so an exception thrown afterwards cannot
  be caught by its catch clause.
includes: [compareArray.js]
features: [generators]
---*/

function* breakLeavesNoLiveCatch() {
  var attempts = 0;
  for (;;) {
    if (++attempts > 3) {
      yield 'WRONG catch re-entered loop';
      return;
    }
    try {
      yield 'in try';
      break;
    } catch (e) {
      yield 'WRONG caught ' + e;
    }
  }
  throw 'after loop';
}

var it = breakLeavesNoLiveCatch();
assert.sameValue(it.next().value, 'in try', 'first step');
var thrown;
try {
  it.next();
  thrown = 'nothing thrown';
} catch (e) {
  thrown = e;
}
assert.sameValue(thrown, 'after loop', 'throw after the loop propagates out of next()');
assert.sameValue(it.next().done, true, 'generator is closed');

function* continueLeavesNoLiveCatch() {
  var attempts = 0;
  for (var i = 0; i < 2; i++) {
    if (++attempts > 5) {
      yield 'WRONG catch re-entered loop';
      return;
    }
    try {
      yield 'in try ' + i;
      continue;
    } catch (e) {
      yield 'WRONG caught ' + e;
    }
  }
  throw 'after loop';
}

it = continueLeavesNoLiveCatch();
assert.sameValue(it.next().value, 'in try 0', 'continue: first step');
assert.sameValue(it.next().value, 'in try 1', 'continue: second step');
thrown = undefined;
try {
  it.next();
  thrown = 'nothing thrown';
} catch (e) {
  thrown = e;
}
assert.sameValue(thrown, 'after loop', 'continue: throw after the loop propagates');

function* finallyRunsOnceThroughCatchOnlyInner() {
  var log = [];
  for (var i = 0; i < 1; i++) {
    try {
      try {
        yield 'in try';
        break;
      } catch (e) {
        log.push('WRONG inner catch');
      }
    } finally {
      log.push('finally');
    }
  }
  yield log.join();
}
it = finallyRunsOnceThroughCatchOnlyInner();
assert.sameValue(it.next().value, 'in try', 'catch-only inner: first step');
assert.sameValue(it.next().value, 'finally', 'the enclosing finally runs exactly once');
assert.sameValue(it.next().done, true, 'catch-only inner: done');

function* labeledBreakPopsEveryContext() {
  var log = [];
  outer: for (var i = 0; i < 1; i++) {
    for (var j = 0; j < 1; j++) {
      try {
        yield 'in try';
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
  yield log.join();
}
it = labeledBreakPopsEveryContext();
assert.sameValue(it.next().value, 'in try', 'labelled: first step');
assert.sameValue(it.next().value, 'caught later', 'labelled: a later try/catch works normally');
