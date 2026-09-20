/*---
description: >
  KNOWN LIMITATION, pinned deliberately: when a generator's class
  declaration has both a heritage expression that throws (a non-suspending
  expression, evaluated inline) and a computed element key that needs
  hoisting out to its own generator state (because it contains a `yield`),
  the hoisted key's side effect currently runs -- and the generator
  suspends on its yield -- before the heritage expression is ever
  evaluated. Spec order requires the opposite: ClassDefinitionEvaluation
  evaluates ClassHeritage (and throws if it is not a constructor) strictly
  before evaluating any ClassElementName.

  This is a known gap in this fix, not a claim that the interpreter is
  spec-compliant here: fully closing it requires making
  ClassDefinitionEvaluation itself resumable/interleaved with the
  generator state machine, rather than independently hoisting each
  suspending sub-expression. This test exists so a future, accidental
  change to this ordering is caught and made a deliberate decision instead
  of silent drift. See jsse issue #625.

  The gap is broader than the heritage-throws case pinned below. Because
  only suspending sub-expressions are hoisted: (1) any non-suspending
  heritage or earlier computed key runs after a later suspending key
  (side-effect order, and a computed key with a side-effecting counter is
  bound to the wrong method); (2) a hoisted key's ToPropertyKey is deferred to
  class-definition time, so a throwing toString on a resumed key value is
  observed after later keys' expressions and yields instead of before them;
  (3) hoisted expressions are evaluated outside the class scope, so a named
  class expression's inner binding is not seen (no TDZ ReferenceError).
  Only case (heritage) is pinned by the assertion below; when any of these is
  fixed, this file's expected order must be revisited deliberately.
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

var r1 = it.next();
order.push("r1:" + JSON.stringify(r1));

var threw = null;
try {
  it.next();
} catch (e) {
  threw = e;
}
order.push(threw instanceof TypeError ? "r2-threw:TypeError" : "r2-no-throw");

assert.compareArray(order, [
  "key-side-effect",
  'r1:{"value":"k","done":false}',
  "heritage-evaluated",
  "r2-threw:TypeError",
]);
