/*---
description: >
  When a generator's class declaration has both a heritage expression that
  throws and a computed element key containing a `yield`, the heritage is
  evaluated -- and throws -- before the key's side effect runs and before the
  generator suspends on the key's `yield`. ClassDefinitionEvaluation evaluates
  ClassHeritage strictly before evaluating any ClassElementName. The class is
  lowered by capturing the heritage and each computed key, in order, up to the
  last suspending one.
esid: sec-runtime-semantics-classdefinitionevaluation
info: |
  Runtime Semantics: ClassDefinitionEvaluation (15.7.14) evaluates
  ClassHeritage (and throws a TypeError if the result is not a
  constructor) before the loop that evaluates each ClassElementName's
  PropertyName.
includes: [compareArray.js]
features: [generators, computed-property-names]
---*/

var order = [];

function* g() {
  class C extends (function () {
    order.push("heritage-evaluated");
    throw new TypeError("bad heritage");
  })() {
    [(order.push("key-side-effect"), yield "k")]() {}
  }
  order.push("after-class");
}

var it = g();

var threw = null;
try {
  it.next();
} catch (e) {
  threw = e;
}
order.push(threw instanceof TypeError ? "threw:TypeError" : "no-throw");

assert.compareArray(order, [
  "heritage-evaluated",
  "threw:TypeError",
]);
