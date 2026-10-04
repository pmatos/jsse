/*---
description: >
  A return intercepted by an outer finally in a plain async function is owned
  by that finally's own try context. A nested try/finally entered inside the
  outer finally's body must run to its own completion, including any
  statements after it, before the outer finally's own completion restores
  the pending return; a suspending finalizer that merely breaks out of its
  own internal loop must not lose the return it is running for.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  Evaluation of a try-finally statement evaluates the Finally clause for
  every completion of its try Block. If the Finally clause completes normally
  the original completion (here a return) is restored; a nested try/finally
  entered inside the Finally clause has its own, independent Completion
  Record and must not be able to consume or observe the outer one.
flags: [async]
includes: [compareArray.js, asyncHelpers.js]
features: [async-functions]
---*/

var log;

async function nestedNormal() {
  try {
    await 0;
    return 'A';
  } finally {
    try {
      await 0;
    } finally {
      log.push('inner');
    }
    log.push('after');
  }
}

async function whileBreak() {
  try {
    return 42;
  } finally {
    while (true) {
      await 0;
      break;
    }
  }
}

async function whileContinue() {
  try {
    return 42;
  } finally {
    for (var i = 0; i < 2; i++) {
      await 0;
      continue;
    }
  }
}

async function main() {
  log = [];
  var value = await nestedNormal();
  assert.sameValue(value, 'A', 'the outer pending return survives the nested try/finally');
  assert.compareArray(
    log,
    ['inner', 'after'],
    'the nested finally and the statement after it both run before the pending return is restored'
  );

  assert.sameValue(
    await whileBreak(),
    42,
    'an internal break that stays inside the finalizer does not lose the pending return'
  );

  assert.sameValue(
    await whileContinue(),
    42,
    'an internal continue that stays inside the finalizer does not lose the pending return'
  );
}

asyncTest(main);
