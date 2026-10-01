/*---
description: >
  A catch parameter's destructuring-default name bound via a suspending
  (`await`-containing) default is visible inside the catch body and not
  visible after the try/catch statement -- the strip-to-temp rewrite used to
  route the pattern through the suspension machinery must not leak the
  binding into the surrounding function scope or change its `catchEnv`
  lifetime.
esid: sec-runtime-semantics-catchclauseevaluation
info: |
  CatchClauseEvaluation

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
flags: [async]
features: [async-functions, destructuring-binding]
---*/

async function f() {
  try {
    throw {};
  } catch ({ a = await 5 }) {
    if (a !== 5) {
      throw new Error('binding not visible (or wrong value) inside catch body');
    }
  }
  if (typeof a !== 'undefined') {
    throw new Error('catch parameter leaked past the try/catch into function scope');
  }
  return 'ok';
}

f().then(function (result) {
  assert.sameValue(result, 'ok');
}).then($DONE, $DONE);
