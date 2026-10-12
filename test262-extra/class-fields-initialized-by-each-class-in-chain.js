/*---
description: >
  Constructing a subclass installs the fields and private methods of every
  class in the chain, each once. super() initializes the elements of the
  class whose constructor made the call (the active function), not those of
  new.target, so a middle class's fields are not skipped and a leaf class's
  private names are not added twice.
esid: sec-super-keyword-runtime-semantics-evaluation
info: |
  SuperCall : super Arguments (13.3.7.1)
    ...
    10. Let F be thisER.[[FunctionObject]].
    11. Assert: F is an ECMAScript function object.
    12. Perform ? InitializeInstanceElements(result, F).

  The default derived constructor (ClassDefinitionEvaluation 15.7.14,
  step 14.a) likewise performs InitializeInstanceElements(result, F) with
  F the class being defined.
includes: [compareArray.js]
features: [class-fields-public, class-fields-private, class-methods-private, class-fields-private-in]
---*/

class A {
  constructor() {
    this.a = 1;
  }
}

class ExplicitB extends A {
  b = "b";
  constructor() {
    super();
  }
}
class DefaultB extends A {
  b = "b";
}

class ExplicitBDefaultC extends ExplicitB {
  c = "c";
}
assert.compareArray(Object.keys(new ExplicitBDefaultC()), ["a", "b", "c"], "explicit middle, default leaf");

class ExplicitBExplicitC extends ExplicitB {
  c = "c";
  constructor() {
    super();
  }
}
assert.compareArray(Object.keys(new ExplicitBExplicitC()), ["a", "b", "c"], "explicit middle, explicit leaf");

class DefaultBExplicitC extends DefaultB {
  c = "c";
  constructor() {
    super();
  }
}
assert.compareArray(Object.keys(new DefaultBExplicitC()), ["a", "b", "c"], "default middle, explicit leaf");

class DefaultBDefaultC extends DefaultB {
  c = "c";
}
assert.compareArray(Object.keys(new DefaultBDefaultC()), ["a", "b", "c"], "default middle, default leaf");

class Four extends ExplicitBDefaultC {
  d = "d";
  constructor() {
    super();
  }
}
assert.compareArray(Object.keys(new Four()), ["a", "b", "c", "d"], "four-level chain");

var order = [];
class Base {
  base = order.push("Base");
}
class Middle extends Base {
  middle = order.push("Middle");
  constructor() {
    order.push("Middle before super");
    super();
    order.push("Middle after super");
  }
}
class Leaf extends Middle {
  leaf = order.push("Leaf");
  constructor() {
    super();
    order.push("Leaf after super");
  }
}
new Leaf();
assert.compareArray(order, [
  "Middle before super",
  "Base",
  "Middle",
  "Middle after super",
  "Leaf",
  "Leaf after super"
], "each class's initializers run once, right after its own super() returns");

class PrivateLeaf extends ExplicitB {
  #p = 1;
  getP() {
    return this.#p;
  }
}
assert.sameValue(new PrivateLeaf().getP(), 1, "a leaf's private field is added once");

class M1 {
  #m1() {}
  static has(o) {
    return #m1 in o;
  }
}
class M2 extends M1 {
  #m2() {}
  constructor() {
    super();
  }
  static has(o) {
    return #m2 in o;
  }
}
class M3 extends M2 {}
var m3 = new M3();
assert(M1.has(m3), "base private method installed");
assert(M2.has(m3), "middle private method installed");

class Override {
  constructor() {
    return { override: true };
  }
}
class OverrideMiddle extends Override {
  f = "f";
}
class OverrideLeaf extends OverrideMiddle {
  g = "g";
}
assert.compareArray(
  Object.keys(new OverrideLeaf()),
  ["override", "f", "g"],
  "fields of every class land on the object a base constructor returned"
);
