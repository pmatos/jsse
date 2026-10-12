// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-getnewtarget
description: >
  new.target is undefined for [[Call]], the constructor's newTarget for
  [[Construct]], inherited lexically by arrow functions, and reset to
  undefined by a plain call made from inside a constructor body.
info: |
  9.4.5 GetNewTarget ( )
    1. Let envRec be GetThisEnvironment().
    3. Return envRec.[[NewTarget]].
  13.3.12.1 Runtime Semantics: Evaluation
    MetaProperty : new . target
      1. Return GetNewTarget().
features: [new.target, Reflect.construct, arrow-function]
---*/

function F() { return new.target; }
function G() {}

assert.sameValue(F(), undefined, "plain call");
assert.sameValue(F.call({}), undefined, "call() with object this");
assert.sameValue(new F(), F, "new F returns new.target (an object)");

var seen;
function Ctor() { seen = new.target; }
new Ctor();
assert.sameValue(seen, Ctor, "new F");
Reflect.construct(Ctor, [], G);
assert.sameValue(seen, G, "Reflect.construct with newTarget");

function ArrowInCtor() {
  var f = () => new.target;
  this.viaArrow = f();
}
assert.sameValue(new ArrowInCtor().viaArrow, ArrowInCtor, "arrow inherits from constructor");

function ArrowInCall() {
  var f = () => new.target;
  return f();
}
assert.sameValue(ArrowInCall(), undefined, "arrow created in plain call");

var escaped;
function MakeArrow() { escaped = () => new.target; }
new MakeArrow();
assert.sameValue(escaped(), MakeArrow, "escaped arrow keeps captured new.target");

var inner;
function Outer() {
  inner = [];
  inner.push(new.target);
  (function () { inner.push(new.target); })();
  inner.push(new.target);
}
new Outer();
assert.sameValue(inner[0], Outer, "before nested call");
assert.sameValue(inner[1], undefined, "plain call inside constructor resets");
assert.sameValue(inner[2], Outer, "restored after nested call");

class A { constructor() { this.nt = new.target; } }
class B extends A {}
assert.sameValue(new B().nt, B, "derived class passes new.target through super()");
