/*---
description: >
  An anonymous class expression whose heritage contains a `yield` is hoisted
  into the generator's state machine. Hoisting must not change the class's
  own `name`: outside a NamedEvaluation position an anonymous class expression
  has the empty string as its name, and the internal temporary the engine
  stores the class in must not leak into it.
esid: sec-runtime-semantics-classdefinitionevaluation
info: |
  ClassExpression : class ClassTail
    1. Let value be ? ClassDefinitionEvaluation of ClassTail with arguments
       undefined and "".
    2. Set value.[[SourceText]] to the source text matched by ClassExpression.

  A variable declaration initializer is an anonymous function definition
  position, so `let C = class ...` names the class "C".
features: [generators, class]
---*/

class Base {}

function* returnedName() {
  yield "pre";
  return (class extends (yield "h") {}).name;
}

var it = returnedName();
it.next();
it.next();
assert.sameValue(it.next(Base).value, "", "class expression in a return position is unnamed");

function nameOf(c) {
  return c.name;
}

function* argumentName() {
  yield "pre";
  return nameOf(class extends (yield "h") {});
}

it = argumentName();
it.next();
it.next();
assert.sameValue(it.next(Base).value, "", "class expression passed as an argument is unnamed");

function* keyName() {
  yield "pre";
  class A {
    [(class extends (yield "h") {}).name]() {}
  }
  return Object.getOwnPropertyNames(A.prototype).join();
}

it = keyName();
it.next();
it.next();
assert.sameValue(it.next(Base).value, "constructor,", "an unnamed class yields the empty-string key");

function* initializerName() {
  let C = class extends (yield "h") {};
  return C.name;
}

it = initializerName();
it.next();
assert.sameValue(it.next(Base).value, "C", "a variable initializer names the class");
