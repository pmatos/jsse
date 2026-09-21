// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-try-statement-runtime-semantics-evaluation
description: >
  An `await using` declared directly in a try, catch or finally clause's own
  statement list (no extra `{ }`) disposes at that clause's own exit, before
  a `finally` runs and before the try statement's own completion propagates —
  not deferred to function exit.
info: |
  TryStatement : try Block Finally

  1. Let B be Completion(Evaluation of Block).
  2. If B.[[Type]] is normal, let F be Completion(Evaluation of Finally).
  [...]
  4. Return ? UpdateEmpty(F, B).

  TryStatement : try Block Catch Finally

  1. Let B be Completion(Evaluation of Block).
  2. If B.[[Type]] is throw, let C be Completion(CatchClauseEvaluation of Catch with argument B.[[Value]]).
  3. Else, let C be B.
  4. Let F be Completion(Evaluation of Finally).
  [...]
  6. Return ? UpdateEmpty(F, C).

  Each clause's own `Block`/`Catch` evaluation (`sec-block-runtime-semantics-evaluation`,
  `sec-runtime-semantics-catchclauseevaluation`) disposes resources declared
  directly in its own StatementList as part of producing B/C — before step 2
  ever evaluates Finally. A `finally` clause is not the disposal boundary; the
  clause's own completion is.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management]
---*/

function resource(log, name) {
  return {
    async [Symbol.asyncDispose]() {
      log.push('dispose-' + name);
    }
  };
}

asyncTest(async function () {
  var log = [];
  await (async function () {
    try {
      await using a = resource(log, 'a');
      log.push('t');
      await 0;
    } finally {
      log.push('fin');
    }
  })();
  assert.compareArray(
    log,
    ['t', 'dispose-a', 'fin'],
    'a resource declared directly in a try body disposes before finally runs'
  );

  log = [];
  await (async function () {
    try {
      throw 0;
    } catch (e) {
      await using a = resource(log, 'a');
      log.push('caught');
    } finally {
      log.push('fin');
    }
  })();
  assert.compareArray(
    log,
    ['caught', 'dispose-a', 'fin'],
    'a resource declared directly in a catch body disposes before finally runs'
  );

  log = [];
  await (async function () {
    try {
      log.push('t');
    } finally {
      await using a = resource(log, 'a');
      log.push('f');
    }
  })();
  log.push('after');
  assert.compareArray(
    log,
    ['t', 'f', 'dispose-a', 'after'],
    'a resource declared directly in a finally body disposes before the try statement completes'
  );

  log = [];
  var caught;
  try {
    await (async function () {
      try {
        await using a = resource(log, 'a');
        throw new Test262Error('from-try');
      } finally {
        log.push('fin');
      }
    })();
  } catch (e) {
    caught = e;
  }
  assert.sameValue(caught && caught.message, 'from-try', 'the throw survives disposal and finally');
  assert.compareArray(
    log,
    ['dispose-a', 'fin'],
    'a throw from the try body still disposes before finally runs'
  );
});
