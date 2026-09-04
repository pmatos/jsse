/*---
description: >
  Await in a for-of nested inside another for-of preserves every active
  per-iteration lexical environment.
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
info: |
  ForIn/OfBodyEvaluation saves the running execution context's
  LexicalEnvironment as oldEnv. Each lexical iteration creates a new
  declarative environment whose outer environment is oldEnv, evaluates the
  loop body in that environment, and restores oldEnv after the body.

  Await suspends and later resumes the async execution context. Nested
  for-of loops must therefore retain the complete chain of active iteration
  environments, including declarations made in the outer loop body.
flags: [async]
includes: [compareArray.js]
features: [async-functions]
---*/

var captures = [];
var afterInnerBreak = [];

async function exercise() {
  var observed = [];

  for (const outer of ['a', 'b']) {
    const record = { outer: outer };

    // Reading `outer` in the inner RHS checks that ForIn/OfHeadEvaluation uses
    // the outer loop's current iteration environment.
    for (const inner of [outer + '1', outer + '2']) {
      await null;
      observed.push(outer + ':' + record.outer + ':' + inner);
    }

    // A non-lexical inner head still has to execute its body in the active
    // outer iteration environment.
    for (var ignored of [0]) {
      await null;
    }
    assert.sameValue(outer, record.outer, 'outer binding survives a var-headed inner loop');

    captures.push(function () {
      return outer + ':' + record.outer;
    });
  }

  // A transformed break jumps directly to the inner loop's after-state. The
  // async driver must pop that loop's environment and reveal the outer one.
  for (const outer of ['break-a', 'break-b']) {
    for (const inner of [1, 2]) {
      await null;
      break;
    }
    afterInnerBreak.push(outer);
  }

  return observed;
}

exercise().then(function (observed) {
  assert.compareArray(
    observed,
    ['a:a:a1', 'a:a:a2', 'b:b:b1', 'b:b:b2'],
    'nested loop bodies resolve through the active outer iteration'
  );
  assert.sameValue(captures[0](), 'a:a', 'first outer iteration has a fresh environment');
  assert.sameValue(captures[1](), 'b:b', 'second outer iteration has a fresh environment');
  assert.compareArray(
    afterInnerBreak,
    ['break-a', 'break-b'],
    'breaking the inner loop restores the outer iteration environment'
  );
}).then($DONE, $DONE);
