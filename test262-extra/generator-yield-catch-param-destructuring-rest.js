/*---
description: >
  A `yield` inside the default of a property ahead of a trailing object
  rest, in a catch clause's parameter pattern, suspends the generator and
  resumes using the sent value -- the catch-parameter desugar
  (`catch ($tmp) { let <pattern> = $tmp; ... }`) turns the parameter into
  an ordinary `let` declaration, which reaches the same `Declaration`-form
  lowering as any other suspending object-rest pattern (issue #771). Also
  covers the same non-idempotent-getter correctness property with no rest
  at all: a catch parameter's own yield-defaulted property must suspend and
  resume without ever being replayed, independent of whether a trailing
  rest is present.
esid: sec-runtime-semantics-catchclauseevaluation
info: |
  Catch : catch ( CatchParameter ) Block

  1. Let oldEnv be the running execution context's LexicalEnvironment.
  2. Let catchEnv be NewDeclarativeEnvironment(oldEnv).
  3. For each element argName of the BoundNames of CatchParameter, do
     a. Perform ! catchEnv.CreateMutableBinding(argName, false).
  4. Set the running execution context's LexicalEnvironment to catchEnv.
  5. Let status be Completion(BindingInitialization of CatchParameter with
     thrownValue and catchEnv).
  ...

  Nothing in BindingInitialization restricts a `yield` expression from
  appearing in a property default, so a generator must suspend at it, and
  the getter for any already-consumed property must be invoked exactly
  once -- never replayed on resume.
features: [generators, destructuring-binding, object-rest]
---*/

function* basicSuspendAndResume() {
  try {
    throw { c: 3 };
  } catch ({ a = yield 1, ...rest }) {
    return { a: a, rest: rest };
  }
}
var it1 = basicSuspendAndResume();
var r1 = it1.next();
assert.sameValue(r1.value, 1, "the catch parameter's default yield suspends the generator");
assert.sameValue(r1.done, false, 'the generator has not completed yet');
var r2 = it1.next(77);
assert.sameValue(r2.done, true, 'the generator completes on resume');
assert.sameValue(r2.value.a, 77, 'the sent value is used for the default');
assert.sameValue(r2.value.rest.c, 3, 'rest keeps properties not named by the pattern');

var calls = 0;
function* nonIdempotentGetter() {
  try {
    throw {
      get a() {
        calls += 1;
        return calls === 1 ? undefined : 42;
      },
      c: 3
    };
  } catch ({ a = yield 1, ...rest }) {
    return { a: a, rest: rest };
  }
}
var it2 = nonIdempotentGetter();
it2.next();
var r3 = it2.next(99);
assert.sameValue(
  r3.value.a,
  99,
  'the value sent to .next() is used for the default, not discarded by a replay'
);
assert.sameValue(calls, 1, "the catch value's getter is called exactly once, never replayed");

var callsNoRest = 0;
function* nonIdempotentGetterNoRest() {
  try {
    throw {
      get a() {
        callsNoRest += 1;
        return callsNoRest === 1 ? undefined : 42;
      }
    };
  } catch ({ a = yield 1 }) {
    return a;
  }
}
var it3 = nonIdempotentGetterNoRest();
it3.next();
var r4 = it3.next(55);
assert.sameValue(
  r4.value,
  55,
  'with no trailing rest at all, the sent value is still used for the default'
);
assert.sameValue(
  callsNoRest,
  1,
  "the catch value's getter is called exactly once even without a rest property"
);
