/*---
description: >
  A rejecting promise awaited by an *array* catch-parameter's destructuring
  default rejects the enclosing async function's own promise, and a
  `finally` block that follows the try/catch still runs before that
  rejection propagates -- mirroring the object-pattern case (issue #773).
  (A live, not-yet-exhausted array-pattern iterator's own closing on a
  rejecting default is covered separately, at the for-of-head site, by
  async-function-forof-array-destructuring-default-await-unwind.js's
  viaReject.)
esid: sec-runtime-semantics-catchclauseevaluation
info: |
  CatchClauseEvaluation

  ...
  5. Let status be Completion(BindingInitialization of CatchParameter with
     arguments thrownValue and catchEnv).
  6. If status is an abrupt completion, then
    a. Set the running execution context's LexicalEnvironment to oldEnv.
    b. Return ? status.
  ...
flags: [async]
includes: [compareArray.js]
features: [async-functions, destructuring-binding]
---*/

var log = [];

async function f() {
  try {
    throw [];
  } catch ([a = await Promise.reject(new Error('rejected-default'))]) {
    log.push('unreachable-catch-body');
  } finally {
    log.push('finally');
  }
}

f().then(
  function () {
    throw new Error('should have rejected');
  },
  function (e) {
    log.push('rejected:' + e.message);
  }
).then(function () {
  assert.compareArray(log, ['finally', 'rejected:rejected-default']);
}).then($DONE, $DONE);
