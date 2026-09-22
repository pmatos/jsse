/*---
description: >
  A `yield` inside the default (Initializer) of a catch parameter's
  destructuring pattern suspends the (synchronous) generator instead of
  silently running to completion with the binding stuck uninitialized, both
  when the try/catch is the only construct in the function (so it stays on
  the tree-walker's native-statement path) and when some other `yield`
  elsewhere in the function forces the try/catch through the compiled
  state-machine's dedicated `catch` lowering.
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
  expression from appearing in a SingleNameBinding's Initializer, so a
  generator must suspend at it like any other `yield`; the catch Block must
  not evaluate (step 7) until that binding initialization completes.
features: [generators, destructuring-binding]
---*/

function* soleConstruct() {
  try {
    throw {};
  } catch ({ a = yield 1 }) {
    return a;
  }
}
var it1 = soleConstruct();
var r1 = it1.next();
assert.sameValue(r1.value, 1, 'sole try/catch: catch param default yield suspends');
assert.sameValue(r1.done, false, 'sole try/catch: has not completed after first yield');
var r2 = it1.next(5);
assert.sameValue(r2.value, 5, 'sole try/catch: catch param default yield resumes with sent value');
assert.sameValue(r2.done, true, 'sole try/catch: resumes to completion');

function* forcedThroughStateMachine() {
  yield 0;
  try {
    yield 'in-try';
    throw {};
  } catch ({ a = yield 1 }) {
    return a;
  }
}
var it2 = forcedThroughStateMachine();
assert.sameValue(it2.next().value, 0, 'state-machine try/catch: leading yield');
assert.sameValue(it2.next().value, 'in-try', 'state-machine try/catch: yield inside try block');
var r3 = it2.next();
assert.sameValue(r3.value, 1, 'state-machine try/catch: catch param default yield suspends');
assert.sameValue(r3.done, false, 'state-machine try/catch: has not completed after catch param yield');
var r4 = it2.next(7);
assert.sameValue(r4.value, 7, 'state-machine try/catch: catch param default yield resumes with sent value');
assert.sameValue(r4.done, true, 'state-machine try/catch: resumes to completion');

function* arrayPatternParam() {
  try {
    throw [];
  } catch ([a = yield 1]) {
    return a;
  }
}
var it3 = arrayPatternParam();
it3.next();
assert.sameValue(it3.next(9).value, 9, 'array-pattern catch param default yield resumes with sent value');

function* closureOverParam() {
  yield 0;
  try {
    throw {};
  } catch ({ a = yield 1 }) {
    return (() => a)();
  }
}
var it4 = closureOverParam();
it4.next();
it4.next();
assert.sameValue(
  it4.next(42).value,
  42,
  'a closure created in the catch body sees the resumed catch param binding'
);

var siblingCount = 0;
function* siblingBindingUnaffected() {
  try {
    throw { b: 3 };
  } catch ({ a = yield 1, b }) {
    siblingCount++;
    return [a, b];
  }
}
var it5 = siblingBindingUnaffected();
it5.next();
var r5 = it5.next(6);
assert.sameValue(r5.value[0], 6, 'sibling property: default-bearing name resumes with sent value');
assert.sameValue(r5.value[1], 3, 'sibling property: non-default name keeps its thrown-value property');
assert.sameValue(siblingCount, 1, 'catch body runs exactly once after the param finishes binding');
