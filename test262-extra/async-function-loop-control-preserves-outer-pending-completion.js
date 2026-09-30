/*---
description: >
  A break or continue intercepted by a finally in a plain async function is
  owned by that finally's own try context. A nested try/finally entered
  inside the outer finally's body — completing normally — must run to its
  own completion, including any statements after it, before the outer
  finally's own completion resumes the pending jump. A jump that leaves the
  finalizer still correctly replaces the pending jump it interrupted.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  Evaluation of a try-finally statement evaluates the Finally clause for
  every completion of its try Block. If the Finally clause completes normally
  the original completion (here a break or continue) is restored; a nested
  try/finally entered inside the Finally clause has its own, independent
  Completion Record and must not be able to consume or observe the outer one.
flags: [async]
includes: [compareArray.js, asyncHelpers.js]
features: [async-functions]
---*/

var log;

async function breakStaysInside() {
  for (;;) {
    try {
      await 0;
      break;
    } finally {
      try {
        await 0;
      } finally {
        log.push('inner');
      }
      log.push('after');
    }
  }
  log.push('end');
}

async function continueStaysInside() {
  for (var i = 0; i < 2; i++) {
    try {
      await 0;
      continue;
    } finally {
      try {
        await 0;
      } finally {
        log.push('inner' + i);
      }
      log.push('after' + i);
    }
  }
  log.push('end');
}

async function breakReplacesContinue() {
  for (var i = 0; i < 3; i++) {
    try {
      await 0;
      continue;
    } finally {
      await 0;
      break;
    }
  }
  log.push('end');
}

async function main() {
  log = [];
  await breakStaysInside();
  assert.compareArray(
    log,
    ['inner', 'after', 'end'],
    'a break stays inside the finalizer: the nested finally and the statement after it run first'
  );

  log = [];
  await continueStaysInside();
  assert.compareArray(
    log,
    ['inner0', 'after0', 'inner1', 'after1', 'end'],
    'a continue stays inside the finalizer for each iteration'
  );

  log = [];
  await breakReplacesContinue();
  assert.compareArray(
    log,
    ['end'],
    'a break that leaves the finalizer correctly replaces the pending continue'
  );
}

asyncTest(main);
