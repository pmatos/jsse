/*---
description: >
  A Block inside an async function gets its own lexical environment, even
  when the function's state-machine lowering splits the block across an
  await. A shadowing declaration inside the block must not clobber the outer
  binding of the same name, and the block's own environment must be
  discarded when control leaves it abruptly (break/continue/return/throw).
esid: sec-block-runtime-semantics-evaluation
info: |
  Block : { StatementList }

  1. Let oldEnv be the running execution context's LexicalEnvironment.
  2. Let blockEnv be NewDeclarativeEnvironment(oldEnv).
  ...
  5. Set the running execution context's LexicalEnvironment to blockEnv.
  6. Let blockValue be Completion(Evaluation of StatementList).
  7. Set the running execution context's LexicalEnvironment to oldEnv.
  8. Return ? blockValue.

  NOTE: No matter how control leaves the Block the LexicalEnvironment is
  always restored to its former state.
flags: [async]
includes: [compareArray.js]
features: [async-functions]
---*/

async function straightLineShadow() {
  let x = 1;
  let readOuterBefore = () => x;
  {
    let x = 2;
    await 0;
    if (x !== 2) throw new Error('inner block should see its own x === 2');
  }
  if (x !== 1) throw new Error('outer x must be restored to 1 once the block exits');
  if (readOuterBefore() !== 1) {
    throw new Error('a closure captured before the block must keep observing the outer binding');
  }
  return 'ok';
}

async function breakOutOfScopedBlock() {
  let seen = [];
  for (let i = 0; i < 3; i++) {
    {
      let j = i;
      await 0;
      if (j === 1) break;
      seen.push(j);
    }
  }
  return seen;
}

async function continueOutOfScopedBlock() {
  let seen = [];
  for (let i = 0; i < 3; i++) {
    {
      let j = i;
      await 0;
      if (j === 1) continue;
      seen.push(j);
    }
    seen.push('after-' + i);
  }
  return seen;
}

async function returnFromScopedBlock() {
  let x = 'outer';
  {
    let x = 'inner';
    await 0;
    return x;
  }
}

async function throwFromScopedBlockCaughtOutside() {
  let log = [];
  try {
    let x = 'outer';
    {
      let x = 'inner';
      await 0;
      log.push(x);
      throw new Error('boom-' + x);
    }
  } catch (e) {
    log.push(e.message);
  }
  return log;
}

async function catchParamShadowing() {
  let e = 'outer';
  try {
    throw 1;
  } catch (e) {
    await 0;
    if (e !== 1) throw new Error('catch parameter must observe the thrown value');
  }
  return e;
}

straightLineShadow()
  .then(function (result) {
    assert.sameValue(result, 'ok', 'straight-line block shadowing across await is scoped correctly');
    return breakOutOfScopedBlock();
  })
  .then(function (seen) {
    assert.compareArray(seen, [0], 'break out of a scoped block leaves the loop after the first iteration');
    return continueOutOfScopedBlock();
  })
  .then(function (seen) {
    assert.compareArray(
      seen,
      [0, 'after-0', 2, 'after-2'],
      'continue out of a scoped block skips the rest of that iteration, including code after the block'
    );
    return returnFromScopedBlock();
  })
  .then(function (result) {
    assert.sameValue(result, 'inner', 'return from inside a scoped block returns the inner value');
    return throwFromScopedBlockCaughtOutside();
  })
  .then(function (log) {
    assert.compareArray(
      log,
      ['inner', 'boom-inner'],
      'a throw from inside a scoped block is caught outside it, with the block value already read'
    );
    return catchParamShadowing();
  })
  .then(function (result) {
    assert.sameValue(result, 'outer', 'the outer binding survives a same-named catch parameter');
  })
  .then($DONE, $DONE);
