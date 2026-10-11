// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-definefield
description: >
  new.target is allowed in a class field initializer, even in a class at the
  top level of a script, and evaluates to undefined there, in arrow functions
  the initializer creates and in a direct eval it runs, whatever constructor
  is running. Static field initializers and static blocks behave the same.
info: |
  Static Semantics: Contains (8.5.1)
    ClassTail : ClassHeritage_opt { ClassBody }
      ...
      4. Return the result of ComputedPropertyContains of ClassBody with argument symbol.
    Static semantic rules that depend upon substructure generally do not look
    into class bodies except for PropertyNames.

  DefineField ( receiver, fieldRecord ) (7.3.33)
    ...
    3. If initializer is not empty, then
      a. Let initValue be ? Call(initializer, receiver).

  The initializer is a method (ClassFieldDefinitionEvaluation, MakeMethod), and
  [[Call]] evaluates its body with NewTarget undefined. A static block body is
  called the same way (ClassStaticBlockDefinitionEvaluation).
features: [class-fields-public, class-fields-private, class-static-fields-public, class-static-block, Reflect.construct]
---*/

class Base {
  direct = new.target;
  arrow = () => new.target;
  viaEval = eval("new.target");
  #hidden = new.target;
  static hidden(obj) {
    return obj.#hidden;
  }
}

function check(obj, label) {
  assert.sameValue(obj.direct, undefined, label + ": direct");
  assert.sameValue(obj.arrow(), undefined, label + ": arrow function");
  assert.sameValue(obj.viaEval, undefined, label + ": direct eval");
  assert.sameValue(Base.hidden(obj), undefined, label + ": private field");
}

check(new Base(), "new");
check(Reflect.construct(Base, [], function Other() {}), "Reflect.construct with a newTarget");

var seenInConstructor;
class Derived extends Base {
  own = new.target;
  constructor() {
    super();
    seenInConstructor = new.target;
  }
}
var derived = new Derived();
check(derived, "super()");
assert.sameValue(derived.own, undefined, "the derived class's own field");
assert.sameValue(seenInConstructor, Derived, "the constructor itself still sees new.target");

function Outer() {
  this.inner = new Base().direct;
  this.outer = new.target;
}
var outer = new Outer();
assert.sameValue(outer.inner, undefined, "a class constructed inside another constructor");
assert.sameValue(outer.outer, Outer, "the enclosing constructor keeps its new.target");

function makeClass() {
  return class {
    static field = new.target;
    static arrow = () => new.target;
    static {
      this.block = new.target;
    }
  };
}
var Made = new makeClass();
assert.sameValue(Made.field, undefined, "static field initializer");
assert.sameValue(Made.arrow(), undefined, "arrow function in a static field initializer");
assert.sameValue(Made.block, undefined, "static block");
