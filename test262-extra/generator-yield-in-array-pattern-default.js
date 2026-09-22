/*---
description: >
  A `yield` inside the default (Initializer) of a var/let/const array
  destructuring pattern suspends a (synchronous) generator instead of
  silently running to completion, including when the declaration is nested
  inside a loop that must itself become suspend-aware.
esid: sec-runtime-semantics-iteratorbindinginitialization
info: |
  SingleNameBinding : BindingIdentifier Initializer_opt

  1. Let bindingId be StringValue of BindingIdentifier.
  2. Let lhs be ? ResolveBinding(bindingId, environment).
  3. Let v be ? GetValue(v).
  4. If Initializer is present and v is undefined, then
     a. Let defaultValue be ? Evaluation of Initializer.
     b. Let v be ? GetValue(defaultValue).

  Nothing in this algorithm restricts a `yield` expression from appearing as
  the Initializer of a SingleNameBinding, so a generator must suspend at it
  like any other `yield`, whether the binding pattern is an
  ArrayBindingPattern or an ObjectBindingPattern.
features: [generators, destructuring-binding]
---*/

function* varArrayDefault() {
  var [a = yield 1] = [];
  return a;
}
var it1 = varArrayDefault();
var r1 = it1.next();
assert.sameValue(r1.value, 1, 'array pattern default yield suspends: first next() value');
assert.sameValue(r1.done, false, 'array pattern default yield suspends: first next() done');
var r2 = it1.next(5);
assert.sameValue(r2.value, 5, 'array pattern default yield resumes with the sent value');
assert.sameValue(r2.done, true, 'array pattern default yield resumes to completion');

function* nestedDefault() {
  var [a, b = yield] = [1];
  return [a, b];
}
var it2 = nestedDefault();
it2.next();
var r3 = it2.next(9);
assert.sameValue(r3.value[0], 1, 'a present element does not suspend, only the missing default does');
assert.sameValue(r3.value[1], 9, 'nested array pattern default yield resumes with the sent value');

function* loopEnclosedDefault() {
  var seen = [];
  for (var i = 0; i < 2; i++) {
    var [a = yield i] = [];
    seen.push(a);
  }
  return seen;
}
var it3 = loopEnclosedDefault();
var l1 = it3.next();
assert.sameValue(l1.value, 0, 'loop-enclosed array pattern default yields the first iteration value');
assert.sameValue(l1.done, false, 'loop-enclosed array pattern default has not completed after first yield');
var l2 = it3.next('x0');
assert.sameValue(l2.value, 1, 'loop-enclosed array pattern default yields the second iteration value');
assert.sameValue(l2.done, false, 'loop-enclosed array pattern default has not completed after second yield');
var l3 = it3.next('x1');
assert.sameValue(l3.done, true, 'loop-enclosed array pattern default completes after the loop ends');
assert.sameValue(l3.value[0], 'x0', 'first loop iteration bound the value sent on resume');
assert.sameValue(l3.value[1], 'x1', 'second loop iteration bound the value sent on resume');
