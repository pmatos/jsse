/*---
description: >
  A break or continue that leaves a try statement containing a yield or await
  in an async generator discards that try context, so a later throw is not
  caught by its stale catch clause.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  Evaluation of a try statement completes once its Block completes. A break or
  continue completion that leaves it hands control to the enclosing loop; the
  try statement is no longer active, so an exception thrown afterwards cannot
  be caught by its catch clause.
flags: [async]
includes: [compareArray.js, asyncHelpers.js]
features: [async-iteration]
---*/

async function* breakLeavesNoLiveCatch() {
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

async function* continueLeavesNoLiveCatch() {
  var attempts = 0;
  for (var i = 0; i < 2; i++) {
    if (++attempts > 5) {
      yield 'WRONG catch re-entered loop';
      return;
    }
    try {
      await null;
      continue;
    } catch (e) {
      yield 'WRONG caught ' + e;
    }
  }
  throw 'after loop';
}

async function* finallyRunsOnceThroughCatchOnlyInner() {
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

async function* labeledBreakPopsEveryContext() {
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

async function expectRejection(promise, expected, message) {
  var outcome;
  try {
    await promise;
    outcome = 'fulfilled';
  } catch (e) {
    outcome = e;
  }
  assert.sameValue(outcome, expected, message);
}

async function run() {
  var it = breakLeavesNoLiveCatch();
  assert.sameValue((await it.next()).value, 'in try', 'first step');
  await expectRejection(it.next(), 'after loop', 'throw after the loop rejects next()');
  assert.sameValue((await it.next()).done, true, 'generator is closed');

  it = continueLeavesNoLiveCatch();
  await expectRejection(it.next(), 'after loop', 'continue: throw after the loop rejects next()');

  it = finallyRunsOnceThroughCatchOnlyInner();
  assert.sameValue((await it.next()).value, 'in try', 'catch-only inner: first step');
  assert.sameValue((await it.next()).value, 'finally', 'the enclosing finally runs exactly once');
  assert.sameValue((await it.next()).done, true, 'catch-only inner: done');

  it = labeledBreakPopsEveryContext();
  assert.sameValue((await it.next()).value, 'in try', 'labelled: first step');
  assert.sameValue(
    (await it.next()).value,
    'caught later',
    'labelled: a later try/catch works normally'
  );
}

asyncTest(run);
