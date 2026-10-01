/*---
description: >
  A return intercepted by an outer finally in an async generator — whether
  from a body-level `return` statement or an external `.return()` call — is
  owned by that finally's own try context. A nested try/finally entered
  inside the outer finally's body must run to its own completion, including
  any statements after it, before the outer finally's own TryExit restores
  the pending return.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  Evaluation of a try-finally statement evaluates the Finally clause for
  every completion of its try Block. If the Finally clause completes normally
  the original completion (here a return) is restored; a nested try/finally
  entered inside the Finally clause has its own, independent Completion
  Record and must not be able to consume or observe the outer one.
flags: [async]
includes: [compareArray.js, asyncHelpers.js]
features: [async-iteration]
---*/

async function* externalReturn() {
  try {
    yield 0;
  } finally {
    try {
      yield 1;
    } finally {
      log.push('inner');
    }
    log.push('after');
  }
}

async function* bodyReturn() {
  try {
    yield 0;
    return 'BODY';
  } finally {
    try {
      yield 1;
    } finally {
      log.push('body-inner');
    }
    log.push('body-after');
  }
}

var log;

async function main() {
  log = [];
  var it = externalReturn();
  assert.sameValue(
    (await it.next()).value,
    0,
    'external: suspended inside the try block, before any finally runs'
  );
  var result = await it.return(42);
  assert.sameValue(
    result.value,
    1,
    'external: return() enters the outer finally, which yields from the nested try'
  );
  assert.sameValue(
    result.done,
    false,
    'external: the generator is not done while the nested try is still suspended'
  );

  result = await it.next();
  assert.sameValue(result.value, 42, 'external: the outer pending return survives the nested try/finally');
  assert.sameValue(result.done, true, 'external: the generator completes with the pending return value');
  assert.compareArray(
    log,
    ['inner', 'after'],
    'external: the nested finally and the statement after it both run before the pending return is restored'
  );

  log = [];
  it = bodyReturn();
  assert.sameValue((await it.next()).value, 0, 'body: suspended inside the try block');
  var step = await it.next();
  assert.sameValue(
    step.value,
    1,
    'body: the body-level return enters the outer finally, which yields from the nested try'
  );
  assert.sameValue(step.done, false, 'body: not done while the nested try is still suspended');

  step = await it.next();
  assert.sameValue(step.value, 'BODY', 'body: the outer pending return survives the nested try/finally');
  assert.sameValue(step.done, true, 'body: the generator completes with the returned value');
  assert.compareArray(
    log,
    ['body-inner', 'body-after'],
    'body: the nested finally and the statement after it both run before the pending return is restored'
  );
}

asyncTest(main);
