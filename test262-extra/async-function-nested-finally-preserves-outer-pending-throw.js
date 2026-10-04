/*---
description: >
  A throw intercepted by an outer finally in a plain async function is owned
  by that finally's own try context. A nested try/finally entered inside the
  outer finally's body — whether entered abruptly by a throw of its own that
  is caught before it escapes, or entered by plain normal control flow with
  no throw at all — must run to its own completion, including any statements
  after it, before the outer finally's own completion restores and rejects
  with the intercepted value.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  Evaluation of a try-finally statement evaluates the Finally clause for
  every completion of its try Block. If the Finally clause completes normally
  the original completion (here a throw) is restored; a nested try/finally
  entered inside the Finally clause has its own, independent Completion
  Record and must not be able to consume or observe the outer one.
flags: [async]
includes: [compareArray.js, asyncHelpers.js]
features: [async-functions]
---*/

var log;

async function abruptlyEntered() {
  try {
    await 0;
    throw 'OUTER';
  } finally {
    try {
      try {
        await 0;
        throw 'INNER';
      } finally {
        log.push('inner finally');
      }
    } catch (e) {
      log.push('caught ' + e);
    }
    log.push('after inner try');
  }
}

async function normalEntry() {
  try {
    await 0;
    throw 'OUTER';
  } finally {
    try {
      await 0;
    } finally {
      log.push('inner finally');
    }
    log.push('after inner try');
  }
}

async function outcome(promise) {
  try {
    await promise;
    return { completion: 'fulfilled' };
  } catch (e) {
    return { completion: 'rejected', value: e };
  }
}

async function main() {
  log = [];
  var result = await outcome(abruptlyEntered());
  assert.sameValue(
    result.completion,
    'rejected',
    'abruptly-entered nested try/finally: the outer throw survives'
  );
  assert.sameValue(result.value, 'OUTER', 'abruptly-entered: the outer throw value is preserved');
  assert.compareArray(
    log,
    ['inner finally', 'caught INNER', 'after inner try'],
    'abruptly-entered: the nested finally, its catch, and the statement after it all run first'
  );

  log = [];
  result = await outcome(normalEntry());
  assert.sameValue(result.completion, 'rejected', 'normal-entry nested try/finally: the outer throw survives');
  assert.sameValue(result.value, 'OUTER', 'normal-entry: the outer throw value is preserved');
  assert.compareArray(
    log,
    ['inner finally', 'after inner try'],
    'normal-entry: the nested finally and the statement after it both run before the outer throw is restored'
  );
}

asyncTest(main);
