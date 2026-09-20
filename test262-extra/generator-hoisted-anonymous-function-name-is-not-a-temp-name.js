/*---
description: >
  When a generator's `yield` forces a sub-expression into an engine-internal
  temporary, an anonymous function or class stored there must stay unnamed, and
  a user variable that merely looks like an internal temporary must still name
  its initializer.
esid: sec-runtime-semantics-evaluation
info: |
  Only a variable declaration initializer, an assignment to an identifier
  reference, and similar NamedEvaluation positions name an anonymous function
  definition. An array element, a conditional arm and a sequence tail are not
  such positions, so the value keeps the empty-string name.
features: [generators, class]
---*/

function* conditionalArm(pick) {
  var a = [yield 0, pick ? class {} : yield 1];
  return a[1].name;
}

var it = conditionalArm(true);
it.next();
assert.sameValue(it.next("x").value, "", "anonymous class in a conditional arm is unnamed");

function* functionArm(pick) {
  var a = [yield 0, pick ? function () {} : yield 1];
  return a[1].name;
}

it = functionArm(true);
it.next();
assert.sameValue(it.next("x").value, "", "anonymous function in a conditional arm is unnamed");

function* sequenceTail() {
  var a = [yield 0, (yield 1, function () {})];
  return a[1].name;
}

it = sequenceTail();
it.next();
it.next("x");
assert.sameValue(it.next("y").value, "", "anonymous function ending a sequence is unnamed");

class Base {}

function* userNamedLikeTemp() {
  let $a_1 = class extends (yield "h") {};
  return $a_1.name;
}

it = userNamedLikeTemp();
it.next();
assert.sameValue(it.next(Base).value, "$a_1", "a user variable that resembles a temporary names its class");
