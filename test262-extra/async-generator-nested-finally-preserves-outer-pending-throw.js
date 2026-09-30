/*---
description: >
  A throw intercepted by an outer finally in an async generator is owned by
  that finally's own try context. A nested try/finally entered inside the
  outer finally's body must run to its own completion, including any
  statements after it, before the outer finally's own TryExit restores and
  re-throws the intercepted value.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  Evaluation of a try-finally statement evaluates the Finally clause for
  every completion of its try Block. If the Finally clause completes normally
  the original completion (here a throw) is restored; a nested try/finally
  entered inside the Finally clause has its own, independent Completion
  Record and must not be able to consume or observe the outer one.
flags: [async]
includes: [compareArray.js, asyncHelpers.js]
features: [async-iteration]
---*/

var log = [];
async function* g() {
  try {
    yield 0;
    throw 'OUTER';
  } finally {
    try {
      yield 1;
    } finally {
      log.push('inner finally');
    }
    log.push('after inner try');
  }
}

async function main() {
  var it = g();
  assert.sameValue((await it.next()).value, 0, 'first yield, inside the try block');
  assert.sameValue((await it.next()).value, 1, 'second yield, inside the nested try');

  var thrown;
  try {
    await it.next();
  } catch (e) {
    thrown = e;
  }
  assert.sameValue(thrown, 'OUTER', 'the outer throw survives the nested try/finally');
  assert.compareArray(
    log,
    ['inner finally', 'after inner try'],
    'the nested finally and the statement after it both run before the outer throw is restored'
  );
}

asyncTest(main);
