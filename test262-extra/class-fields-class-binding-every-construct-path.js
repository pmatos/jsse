// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-classdefinitionevaluation
description: >
  A field initializer sees the class's own name binding, the inner binding a
  class expression creates, whichever path constructs the class: new,
  Reflect.construct, a subclass's super(), a bound function or a Proxy.
info: |
  ClassDefinitionEvaluation (15.7.14)
    1. Let env be the running execution context's LexicalEnvironment.
    2. Let classEnv be NewDeclarativeEnvironment(env).
    3. If classBinding is not undefined, then
      a. Perform ! classEnv.CreateImmutableBinding(classBinding, true).
    ...
    29. For each ClassElement e of elements, do
      a. If IsStatic of e is false, then
        i. Let element be Completion(ClassElementEvaluation of e with arguments proto and false).

  The field initializer is evaluated as a method whose closure is classEnv, so
  classBinding resolves to F.
features: [class-fields-public, Reflect.construct, Proxy]
---*/

var Named = class Inner {
  self = Inner;
};
assert.sameValue(typeof Inner, "undefined", "the inner name is not visible outside the class");

assert.sameValue(new Named().self, Named, "new");
assert.sameValue(Reflect.construct(Named, []).self, Named, "Reflect.construct");

function OtherTarget() {}
assert.sameValue(Reflect.construct(Named, [], OtherTarget).self, Named, "Reflect.construct with a newTarget");

class DefaultDerived extends Named {}
assert.sameValue(new DefaultDerived().self, Named, "super() from a default constructor");

class ExplicitDerived extends Named {
  constructor() {
    super();
  }
}
assert.sameValue(new ExplicitDerived().self, Named, "super() from an explicit constructor");

assert.sameValue(new (Named.bind(null))().self, Named, "a bound function");
assert.sameValue(new (new Proxy(Named, {}))().self, Named, "a Proxy without a construct trap");
