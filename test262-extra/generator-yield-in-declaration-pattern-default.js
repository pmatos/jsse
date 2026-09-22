/*---
description: >
  A `yield` inside the default (Initializer) of a var/let/const object
  destructuring pattern suspends the (synchronous) generator instead of
  silently running to completion.
esid: sec-runtime-semantics-keyedbindinginitialization
info: |
  SingleNameBinding : BindingIdentifier Initializer_opt

  1. Let bindingId be StringValue of BindingIdentifier.
  2. Let lhs be ? ResolveBinding(bindingId, environment).
  3. If IsAnonymousFunctionDefinition(Initializer) is true, then
     ...
  4. Else,
     a. Let v be ? GetValue(v).
  5. If Initializer is present and v is undefined, then
     a. If IsAnonymousFunctionDefinition(Initializer) is true, then
       ...
     b. Else,
       i. Let defaultValue be ? Evaluation of Initializer.
       ii. Let v be ? GetValue(defaultValue).

  Nothing in this algorithm restricts a `yield` expression from appearing as
  the Initializer of a SingleNameBinding, so a generator must suspend at it
  like any other `yield`.
features: [generators, destructuring-binding]
---*/

function* varObjectDefault() {
  var { a = yield 1 } = {};
  return a;
}
var it1 = varObjectDefault();
var r1 = it1.next();
assert.sameValue(r1.value, 1, 'var pattern default yield suspends: first next() value');
assert.sameValue(r1.done, false, 'var pattern default yield suspends: first next() done');
var r2 = it1.next(5);
assert.sameValue(r2.value, 5, 'var pattern default yield resumes with the sent value');
assert.sameValue(r2.done, true, 'var pattern default yield resumes to completion');

function* letObjectDefault() {
  let { a = yield 1 } = {};
  return a;
}
var it2 = letObjectDefault();
it2.next();
assert.sameValue(it2.next(7).value, 7, 'let pattern default yield resumes with the sent value');

function* constObjectDefault() {
  const { a = yield 1 } = {};
  return a;
}
var it3 = constObjectDefault();
it3.next();
assert.sameValue(it3.next(9).value, 9, 'const pattern default yield resumes with the sent value');

function* computedKeyYield() {
  var { [yield 'k']: a } = { k: 5 };
  return a;
}
var it4 = computedKeyYield();
var r4 = it4.next();
assert.sameValue(r4.value, 'k', 'computed key yield suspends before the key is evaluated');
assert.sameValue(r4.done, false, 'computed key yield has not completed the generator yet');
var r5 = it4.next('k');
assert.sameValue(r5.value, 5, 'computed key yield resumes and looks up the sent key');
assert.sameValue(r5.done, true, 'computed key yield resumes to completion');

var n = 0;
function* sourceEvaluatedOnce() {
  var { a = yield 1 } = (n++, {});
  return a;
}
var it5 = sourceEvaluatedOnce();
it5.next();
it5.next(3);
assert.sameValue(
  n,
  1,
  'the pattern source expression is evaluated exactly once, not replayed on resume'
);
