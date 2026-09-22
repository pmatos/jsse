/*---
description: >
  A `yield` inside the default (Initializer) of a for-of head declaration's
  destructuring pattern suspends an async generator instead of silently
  running to completion, both when the loop is the only construct in the
  function and when another `yield` forces it through the compiled
  state-machine's dedicated loop lowering.
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
info: |
  ForIn/OfBodyEvaluation ( lhs, stmt, iteratorRecord, iterationKind, lhsKind,
  labelSet [ , iteratorKind ] )

  ...
  6. Repeat,
    a. Let nextResult be ? Call(iteratorRecord.[[NextMethod]], iteratorRecord.[[Iterator]]).
    ...
    e. Let nextValue be ? IteratorValue(nextResult).
    ...
    g. Else,
      i. Assert: lhsKind is lexical-binding.
      ii. Assert: lhs is a ForDeclaration.
      iii. Let iterationEnv be NewDeclarativeEnvironment(oldEnv).
      iv. Perform ForDeclarationBindingInstantiation of lhs with argument iterationEnv.
      v. Set the running execution context's LexicalEnvironment to iterationEnv.
      vi. If destructuring is true, then
        1. Let status be Completion(ForDeclarationBindingInitialization of lhs
           with arguments nextValue and iterationEnv).
    ...

  Nothing in ForDeclarationBindingInitialization restricts a `yield`
  expression from appearing in a SingleNameBinding's Initializer, so an async
  generator must suspend at it like any other `yield`.
flags: [async]
features: [async-iteration, destructuring-binding]
---*/

async function run() {
  async function* soleForOf() {
    for (const { a = yield 1, b } of [{ b: 9 }]) {
      return [a, b];
    }
  }
  var it1 = soleForOf();
  var r1 = await it1.next();
  var r2 = await it1.next(6);

  async function* forcedForOf() {
    yield 0;
    for (const { a = yield 1, b } of [{ b: 9 }]) {
      yield 'tick';
      return [a, b];
    }
  }
  var it2 = forcedForOf();
  var s0 = await it2.next();
  var r3 = await it2.next();
  var r4 = await it2.next(6);
  var r5 = await it2.next();

  return [r1, r2, s0, r3, r4, r5];
}

run()
  .then(function ([r1, r2, s0, r3, r4, r5]) {
    assert.sameValue(r1.value, 1, 'sole for-of: head pattern default yield suspends');
    assert.sameValue(r1.done, false, 'sole for-of: has not completed after first yield');
    assert.sameValue(r2.value[0], 6, 'sole for-of: default-bearing name resumes with sent value');
    assert.sameValue(r2.value[1], 9, 'sole for-of: sibling non-default property is preserved');

    assert.sameValue(s0.value, 0, 'state-machine for-of: leading yield');
    assert.sameValue(r3.value, 1, 'state-machine for-of: head pattern default yield suspends');
    assert.sameValue(r3.done, false, 'state-machine for-of: has not completed after head yield');
    assert.sameValue(r4.value, 'tick', 'state-machine for-of: resumes into the loop body');
    assert.sameValue(r5.value[0], 6, 'state-machine for-of: default-bearing name resumes with sent value');
    assert.sameValue(r5.value[1], 9, 'state-machine for-of: sibling non-default property is preserved');
  })
  .then($DONE, $DONE);
