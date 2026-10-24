// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-class-definitions-static-semantics-early-errors
description: >
  A class nested in the body of a private method, accessor or static private
  method may reference a private name of the enclosing class, whatever the
  order in which the enclosing class declares its members.
info: |
  ClassBody : ClassElementList
    It is a Syntax Error if AllPrivateIdentifiersValid of ClassElementList with
    argument the union of names and PrivateBoundIdentifiers of ClassBody is
    false.

  AllPrivateIdentifiersValid is checked against the private names of every
  enclosing class body, including those declared after the nested class.
features: [class-methods-private, class-static-methods-private]
---*/

function accepts(src) {
  try {
    new Function(src);
    return true;
  } catch (e) {
    if (!(e instanceof SyntaxError)) throw e;
    return false;
  }
}

var accepted = [
  "class A { #q0(){ class B { m(){ this.#q0 } } } }",
  "class A { #q0(){ class B { m(){ #q0 in this } } } }",
  "class A { static #q0(){ class B { m(){ A.#q0 } } } }",
  "class A { get #q0(){ class B { m(){ this.#q0 } } } }",
  "class A { set #q0(v){ class B { m(){ this.#q0 } } } }",
  "class A { #q0(){ return class { m(){ this.#q0 } } } }",
  "class A { #q0(){ return () => class { m(){ this.#q0 } } } }",
  "class A { #q0(){ class B { #r(){ class C { m(){ this.#q0; this.#r } } } } } }",
  "class A { m(){ class B { n(){ this.#q0 } } } #q0(){} }",
  "class A { #q0; m(){ class B { n(){ this.#q0 } } } }"
];

var rejected = [
  "class A { #q0(){ class B { m(){ this.#q1 } } } }",
  "class A { m(){ class B { n(){ this.#q0 } } } }",
  "class A { #q0(){ class B { m(){ this.#q1 } } } #q2(){} }",
  "class A { #q0(){} } class B { m(){ this.#q0 } }"
];

accepted.forEach(function (src) {
  assert.sameValue(accepts(src), true, src);
});

rejected.forEach(function (src) {
  assert.sameValue(accepts(src), false, src);
});

class Q {
  #q() {
    return eval("(class B { m() { return this.#q; } })");
  }
  static check() {
    return typeof new Q().#q();
  }
}
assert.sameValue(Q.check(), "function", "eval inside a private method sees #q");

class R {
  #q() {
    eval("class B { m() { return this.#z; } }");
  }
  static run() {
    new R().#q();
  }
}
assert.throws(SyntaxError, function () {
  R.run();
}, "eval inside a private method rejects an undeclared name");
