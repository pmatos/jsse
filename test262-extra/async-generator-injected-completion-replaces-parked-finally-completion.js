/*---
description: >
  A fresh, externally-injected .throw()/.return() delivered while a finally
  is already running on behalf of an earlier, context-owned completion
  replaces that earlier completion — the same override rule as a throw or
  return produced by the finally's own body, exercised instead through
  check_abrupt_on_resume's resume-input path. An outer finally wrapping the
  one that owns the parked completion must still run exactly once, after the
  injected completion's own finally finishes.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  Evaluation of a try-finally statement evaluates the Finally clause for
  every completion of its try Block. If the Finally clause completes
  abruptly (here because the generator was resumed with an externally
  injected throw or return while suspended inside it), that abrupt
  completion replaces the one the Finally clause was running to restore.
flags: [async]
includes: [asyncHelpers.js]
features: [async-iteration]
---*/

var log;

async function* injectedThrowReplacesParkedThrow() {
  try {
    try {
      yield 0;
      throw 'A';
    } finally {
      yield 1;
    }
  } finally {
    log.push('outer finally');
  }
}

async function* injectedReturnReplacesParkedThrow() {
  try {
    try {
      yield 0;
      throw 'A';
    } finally {
      yield 1;
    }
  } finally {
    log.push('outer finally');
  }
}

async function main() {
  log = [];
  var it = injectedThrowReplacesParkedThrow();
  assert.sameValue((await it.next()).value, 0, 'throw case: suspended inside the inner try');
  assert.sameValue(
    (await it.next()).value,
    1,
    'throw case: the inner finally runs, parking the throw on its own context'
  );

  var thrown;
  try {
    await it.throw('B');
  } catch (e) {
    thrown = e;
  }
  assert.sameValue(
    thrown,
    'B',
    'throw case: the injected throw replaces the parked one, not the other way around'
  );
  assert.compareArray(
    log,
    ['outer finally'],
    'throw case: the outer finally runs exactly once, after the inner finally is replaced'
  );

  log = [];
  it = injectedReturnReplacesParkedThrow();
  assert.sameValue((await it.next()).value, 0, 'return case: suspended inside the inner try');
  assert.sameValue(
    (await it.next()).value,
    1,
    'return case: the inner finally runs, parking the throw on its own context'
  );

  var result = await it.return(2);
  assert.sameValue(result.value, 2, 'return case: the injected return replaces the parked throw');
  assert.sameValue(result.done, true, 'return case: the generator completes');
  assert.compareArray(
    log,
    ['outer finally'],
    'return case: the outer finally runs exactly once, after the inner finally is replaced'
  );
}

asyncTest(main);
