/*---
description: >
  A `let` declared inside a `while`/`do-while` body gets a fresh binding on
  every entry into the body, even when the async function's state-machine
  lowering re-enters the same lowered body state on each iteration. Closures
  created in different iterations must observe distinct bindings, not one
  binding shared across the whole loop.
esid: sec-while-statement-runtime-semantics-labelledevaluation
info: |
  IterationStatement : while ( Expression ) Statement

  ...
  d. Let stmtResult be Completion(Evaluation of Statement).
  ...

  The while statement itself performs no environment bookkeeping; a fresh
  binding per iteration comes entirely from re-evaluating Statement (a
  Block) from scratch each pass, per the Block evaluation semantics'
  NewDeclarativeEnvironment on every entry (sec-block-runtime-semantics-evaluation).
flags: [async]
includes: [compareArray.js]
features: [async-functions]
---*/

async function whileLoopClosures() {
  let i = 0;
  let closures = [];
  while (i < 3) {
    let j = i;
    closures.push(() => j);
    await 0;
    i++;
  }
  return closures.map(function (f) { return f(); });
}

async function doWhileLoopClosures() {
  let i = 0;
  let closures = [];
  do {
    let j = i;
    closures.push(() => j);
    await 0;
    i++;
  } while (i < 3);
  return closures.map(function (f) { return f(); });
}

whileLoopClosures()
  .then(function (values) {
    assert.compareArray(values, [0, 1, 2], 'while: each iteration\'s closure keeps its own let binding');
    return doWhileLoopClosures();
  })
  .then(function (values) {
    assert.compareArray(values, [0, 1, 2], 'do-while: each iteration\'s closure keeps its own let binding');
  })
  .then($DONE, $DONE);
