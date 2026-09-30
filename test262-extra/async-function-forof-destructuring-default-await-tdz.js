/*---
description: >
  A `for-of` head's TDZ environment (built to declare the lexical head's
  bound names before the iterable expression is evaluated) must keep seeing
  the *real* pattern even when the head's destructuring default contains an
  `await` and gets rewritten to a trivial temp identifier for the driver's
  per-iteration binding call. A shadowed outer name referenced by the
  iterable expression itself must still throw `ReferenceError` from TDZ,
  both when the loop is the function's only suspension and when an
  unrelated `await` elsewhere forces the compiled state machine anyway. The
  equivalent `yield`-triggered rewrite (already shipped) must keep this
  property too, once both triggers share the same `left`-split fix.
esid: sec-runtime-semantics-forinofheadevaluation
info: |
  ForIn/OfHeadEvaluation ( uninitializedBoundNames, expr, iterationKind )

  ...
  3. If uninitializedBoundNames is not empty, then
    a. Assert: uninitializedBoundNames has no duplicate entries.
    b. Let newEnv be NewDeclarativeEnvironment(oldEnv).
    c. For each String name of uninitializedBoundNames, do
      i. Perform ! newEnv.CreateMutableBinding(name, false).
    d. Set the running execution context's LexicalEnvironment to newEnv.
  4. Let exprRef be Completion(Evaluation of expr).
  ...
flags: [async]
features: [async-functions, generators, destructuring-binding]
---*/

async function sole() {
  let a = 'outer';
  for (let { a = await 1 } of [a]) {
  }
}

async function alongsideUnrelatedAwait() {
  await 0;
  let a = 'outer';
  for (let { a = await 1 } of [a]) {
  }
}

Promise.all([
  sole().then(
    function () { throw new Error('sole: should have thrown ReferenceError'); },
    function (e) { return e; }
  ),
  alongsideUnrelatedAwait().then(
    function () { throw new Error('alongside: should have thrown ReferenceError'); },
    function (e) { return e; }
  ),
]).then(function (errors) {
  assert.sameValue(
    errors[0] instanceof ReferenceError,
    true,
    'sole: shadowed self-reference throws ReferenceError from the head TDZ'
  );
  assert.sameValue(
    errors[1] instanceof ReferenceError,
    true,
    'alongside: shadowed self-reference throws ReferenceError from the head TDZ ' +
      'even when the compiled state machine is already in use for another await'
  );

  // The equivalent yield-triggered rewrite must keep this property too --
  // it shares the same left-split fix.
  function* g() {
    let a = 'outer';
    for (let { a = yield 1 } of [a]) {
    }
  }
  var it = g();
  assert.throws(
    ReferenceError,
    function () { it.next(); },
    'generator: shadowed self-reference throws ReferenceError from the head TDZ (yield trigger)'
  );

  function* forcedG() {
    yield 0;
    let a = 'outer';
    for (let { a = yield 1 } of [a]) {
    }
  }
  var it2 = forcedG();
  it2.next();
  assert.throws(
    ReferenceError,
    function () { it2.next(); },
    'generator: shadowed self-reference throws ReferenceError from the head TDZ ' +
      '(state-machine path, yield trigger)'
  );
}).then($DONE, $DONE);
