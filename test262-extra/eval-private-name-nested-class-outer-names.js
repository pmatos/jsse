// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-performeval
description: >
  Direct eval inside a nested class is parsed with the private environment of
  its caller, which includes the private names of every enclosing class body,
  not only the innermost one.
info: |
  PerformEval ( x, strictCaller, direct )
    ...
    f. If direct is true, then
       i. Let privateIdentifiers be a new empty List.
       ii. Let pointer be runningContext's PrivateEnvironment.
       iii. Repeat, while pointer is not null,
            1. For each Private Name binding of pointer.[[Names]], append
               binding.[[Description]] to privateIdentifiers.
            2. Set pointer to pointer.[[OuterPrivateEnvironment]].
       iv. If AllPrivateIdentifiersValid of body with argument privateIdentifiers
           is false, throw a SyntaxError exception.
features: [class-fields-private]
---*/

class A {
  #x = 1;
  static #sx = 10;
  m() {
    class B {
      #y = 2;
      n() {
        return eval("this.#y");
      }
      outer(a) {
        return eval("a.#x");
      }
      both(a) {
        return eval("a.#x + this.#y");
      }
      staticOuter() {
        return eval("A.#sx");
      }
      undeclared() {
        return eval("this.#z");
      }
    }
    return new B();
  }
}

var a = new A();
var b = a.m();

assert.sameValue(b.n(), 2, "innermost class name is visible");
assert.sameValue(b.outer(a), 1, "outer class name is visible");
assert.sameValue(b.both(a), 3, "outer and inner names are visible together");
assert.sameValue(b.staticOuter(), 10, "outer static private name is visible");
assert.throws(SyntaxError, function () {
  b.undeclared();
}, "undeclared name is still rejected");

class C {
  #c = "c";
  m() {
    return class D {
      #d = "d";
      n() {
        return class E {
          #e = "e";
          p(c) {
            return eval("c.#c + this.#e");
          }
          q() {
            return eval("(function () { return this.#d; })").call(this);
          }
        };
      }
    };
  }
}

var E = new (new C().m())().n();
var e = new E();
assert.sameValue(e.p(new C()), "ce", "names from three enclosing classes resolve");
assert.throws(TypeError, function () {
  e.q();
}, "middle-class name resolves (receiver lacks it, so TypeError not SyntaxError)");
