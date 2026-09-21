/*---
description: >
  A C-style `for` loop with a `let` head gets a fresh per-iteration
  environment (CreatePerIterationEnvironment), even when the async
  function's state-machine lowering splits the test/body/update across an
  await. Closures created in different iterations must observe distinct
  bindings, the copy-forward must happen before the update expression runs
  (so the update mutates the new iteration's copy, not a stale one), and a
  `const` head (no reassignment possible) must still behave correctly.
esid: sec-forbodyevaluation
info: |
  ForBodyEvaluation ( test, increment, stmt, perIterationBindings, labelSet ):

  1. Let V be undefined.
  2. Perform ? CreatePerIterationEnvironment(perIterationBindings).
  3. Repeat,
    a. If test is not [empty], ...
    b. Let result be Completion(Evaluation of stmt).
    ...
    e. Perform ? CreatePerIterationEnvironment(perIterationBindings).
    f. If increment is not [empty], ...
flags: [async]
includes: [compareArray.js]
features: [async-functions]
---*/

async function forLoopLetClosures() {
  let closures = [];
  for (let i = 0; i < 3; i++) {
    closures.push(() => i);
    await 0;
  }
  return closures.map(function (f) { return f(); });
}

async function forLoopConstHeadClosures() {
  let closures = [];
  for (const i of [10, 20, 30]) {
    closures.push(() => i);
    await 0;
  }
  return closures.map(function (f) { return f(); });
}

async function forLoopUpdateSeesFreshCopy() {
  // The update expression (`i++`) must run against the *new* per-iteration
  // copy, not the one the body's closure captured — otherwise the closure
  // and the update would alias the same binding and both would read the
  // post-increment value.
  let closures = [];
  let updateReads = [];
  for (let i = 0; i < 3; i = (updateReads.push(i), i + 1)) {
    closures.push(() => i);
    await 0;
  }
  return {
    closureValues: closures.map(function (f) { return f(); }),
    updateReads: updateReads
  };
}

forLoopLetClosures()
  .then(function (values) {
    assert.compareArray(values, [0, 1, 2], 'for(let ...): each iteration\'s closure keeps its own binding');
    return forLoopConstHeadClosures();
  })
  .then(function (values) {
    assert.compareArray(values, [10, 20, 30], 'for-of with a const head is unaffected (regression guard)');
    return forLoopUpdateSeesFreshCopy();
  })
  .then(function (result) {
    assert.compareArray(
      result.closureValues,
      [0, 1, 2],
      'closures observe the value each iteration copied forward, not the final loop value'
    );
    assert.compareArray(
      result.updateReads,
      [0, 1, 2],
      'the update expression reads the freshly copied-forward value for its own iteration'
    );
  })
  .then($DONE, $DONE);
