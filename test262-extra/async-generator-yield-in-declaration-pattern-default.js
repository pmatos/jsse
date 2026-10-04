/*---
description: >
  A `yield` inside the default (Initializer) of a var/let/const object
  destructuring pattern suspends an async generator instead of silently
  running to completion.
esid: sec-runtime-semantics-keyedbindinginitialization
info: |
  SingleNameBinding : BindingIdentifier Initializer?

  1. Let bindingId be the StringValue of BindingIdentifier.
  2. Let lhs be ? ResolveBinding(bindingId, environment).
  3. Let v be ? GetV(value, propertyName).
  4. If Initializer is present and v is undefined, then
     a. If IsAnonymousFunctionDefinition(Initializer) is true, then
       i. Set v to ? NamedEvaluation of Initializer with argument bindingId.
     b. Else,
       i. Let defaultValue be ? Evaluation of Initializer.
       ii. Set v to ? GetValue(defaultValue).
  5. If environment is undefined, return ? PutValue(lhs, v).
  6. Return ? InitializeReferencedBinding(lhs, v).

  Nothing in this algorithm restricts a `yield` expression from appearing as
  the Initializer of a SingleNameBinding, so an async generator must suspend
  at it like any other `yield`.
flags: [async]
features: [async-iteration, destructuring-binding]
---*/

async function run() {
  async function* g() {
    var { a = yield 1 } = {};
    return a;
  }
  var it = g();
  var r1 = await it.next();
  var r2 = await it.next(5);
  return [r1, r2];
}

run()
  .then(function ([r1, r2]) {
    assert.sameValue(r1.value, 1, 'async generator pattern default yield suspends: value');
    assert.sameValue(r1.done, false, 'async generator pattern default yield suspends: done');
    assert.sameValue(r2.value, 5, 'async generator pattern default yield resumes with sent value');
    assert.sameValue(r2.done, true, 'async generator pattern default yield resumes to completion');
  })
  .then($DONE, $DONE);
