/*---
description: >
  A `yield` inside the default (Initializer) of a catch parameter's
  destructuring pattern suspends an async generator instead of silently
  running to completion with the binding stuck uninitialized, both when the
  try/catch is the only construct in the function and when another `yield`
  forces it through the compiled state-machine's dedicated `catch` lowering.
esid: sec-runtime-semantics-catchclauseevaluation
info: |
  Catch : `catch` `(` CatchParameter `)` Block

  1. Let oldEnv be the running execution context's LexicalEnvironment.
  2. Let catchEnv be NewDeclarativeEnvironment(oldEnv).
  3. For each element argName of the BoundNames of CatchParameter, do
    a. Perform ! catchEnv.CreateMutableBinding(argName, false).
  4. Set the running execution context's LexicalEnvironment to catchEnv.
  5. Let status be Completion(BindingInitialization of CatchParameter with
     arguments thrownValue and catchEnv).
  6. If status is an abrupt completion, then
    a. Set the running execution context's LexicalEnvironment to oldEnv.
    b. Return ? status.
  7. Let B be Completion(Evaluation of Block).
  8. Set the running execution context's LexicalEnvironment to oldEnv.
  9. Return ? B.

  Nothing in CatchParameter's BindingInitialization restricts a `yield`
  expression from appearing in a SingleNameBinding's Initializer, so an
  async generator must suspend at it like any other `yield`.
flags: [async]
features: [async-iteration, destructuring-binding]
---*/

async function run() {
  async function* soleConstruct() {
    try {
      throw {};
    } catch ({ a = yield 1 }) {
      return a;
    }
  }
  var it1 = soleConstruct();
  var r1 = await it1.next();
  var r2 = await it1.next(5);

  async function* forcedThroughStateMachine() {
    yield 0;
    try {
      yield 'in-try';
      throw {};
    } catch ({ a = yield 1 }) {
      return a;
    }
  }
  var it2 = forcedThroughStateMachine();
  var s0 = await it2.next();
  var s1 = await it2.next();
  var r3 = await it2.next();
  var r4 = await it2.next(7);

  return [r1, r2, s0, s1, r3, r4];
}

run()
  .then(function ([r1, r2, s0, s1, r3, r4]) {
    assert.sameValue(r1.value, 1, 'sole try/catch: catch param default yield suspends');
    assert.sameValue(r1.done, false, 'sole try/catch: has not completed after first yield');
    assert.sameValue(r2.value, 5, 'sole try/catch: catch param default yield resumes with sent value');
    assert.sameValue(r2.done, true, 'sole try/catch: resumes to completion');

    assert.sameValue(s0.value, 0, 'state-machine try/catch: leading yield');
    assert.sameValue(s1.value, 'in-try', 'state-machine try/catch: yield inside try block');
    assert.sameValue(r3.value, 1, 'state-machine try/catch: catch param default yield suspends');
    assert.sameValue(r3.done, false, 'state-machine try/catch: has not completed after catch param yield');
    assert.sameValue(r4.value, 7, 'state-machine try/catch: catch param default yield resumes with sent value');
    assert.sameValue(r4.done, true, 'state-machine try/catch: resumes to completion');
  })
  .then($DONE, $DONE);
