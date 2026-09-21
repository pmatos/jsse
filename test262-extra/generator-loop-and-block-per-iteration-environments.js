/*---
description: >
  The same lexical-scope-stack mechanism that fixes async functions
  (per-iteration `let` bindings in while/for loops, block shadowing across a
  suspension, catch-parameter shadowing) applies identically to sync
  generators and async generators, since all three share one state-machine
  lowering (`generator_transform.rs`).
esid: sec-createperiterationenvironment
info: |
  CreatePerIterationEnvironment and Block Evaluation
  (sec-createperiterationenvironment, sec-block-runtime-semantics-evaluation)
  are runtime-semantics clauses independent of whether the enclosing function
  is a plain function, a generator, or an async generator.
flags: [async]
includes: [compareArray.js]
features: [async-functions, async-generators, generators]
---*/

function* forLoopGenerator() {
  let closures = [];
  for (let i = 0; i < 3; i++) {
    closures.push(() => i);
    yield i;
  }
  return closures.map(function (f) { return f(); });
}

function* blockShadowGenerator() {
  let x = 1;
  let readOuterBefore = () => x;
  {
    let x = 2;
    yield x;
  }
  return [x, readOuterBefore()];
}

function* catchParamGenerator() {
  let e = 'outer';
  try {
    throw 1;
  } catch (e) {
    yield e;
  }
  return e;
}

function drive(gen) {
  let result;
  do {
    result = gen.next();
  } while (!result.done);
  return result.value;
}

assert.compareArray(
  drive(forLoopGenerator()),
  [0, 1, 2],
  'sync generator for-loop closures keep distinct per-iteration bindings'
);
assert.compareArray(
  drive(blockShadowGenerator()),
  [1, 1],
  'sync generator block shadowing restores the outer binding once the block exits'
);
assert.sameValue(
  drive(catchParamGenerator()),
  'outer',
  'sync generator catch-parameter shadowing does not clobber the outer binding'
);

async function asyncGenForLoopClosures() {
  async function* g() {
    let closures = [];
    for (let i = 0; i < 3; i++) {
      closures.push(() => i);
      await 0;
      yield i;
    }
    return closures.map(function (f) { return f(); });
  }
  let it = g();
  let result;
  do {
    result = await it.next();
  } while (!result.done);
  return result.value;
}

asyncGenForLoopClosures()
  .then(function (values) {
    assert.compareArray(
      values,
      [0, 1, 2],
      'async generator for-loop closures keep distinct per-iteration bindings'
    );
  })
  .then($DONE, $DONE);
