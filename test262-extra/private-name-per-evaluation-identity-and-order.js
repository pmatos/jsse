// Copyright (C) 2026 jsse contributors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
description: >
  A Private Name is created per class evaluation and per source name: two
  evaluations of one class body, and two classes that spell the same `#x`,
  never share a private element. Every access form (read, write, compound
  assignment, update, method call, `#x in o`) resolves through the Private
  Environment of the class that lexically encloses it, and a compound
  assignment reads the private element before it evaluates the right-hand side.
esid: sec-privatefieldadd
features: [class, class-fields-private, class-methods-private, class-static-fields-private]
info: |
  15.7.14 Runtime Semantics: ClassDefinitionEvaluation
    Step: For each String dn of the PrivateBoundIdentifiers of ClassBody,
    create a new Private Name and add it to the class's PrivateEnvironment.

  13.15.2 AssignmentExpression : LeftHandSideExpression AssignmentOperator AssignmentExpression
    3.a. Let lref be ? Evaluation of LeftHandSideExpression.
    3.b. Let lval be ? GetValue(lref).
    3.d. Let rref be ? Evaluation of AssignmentExpression.

  13.10.1 RelationalExpression : PrivateIdentifier in ShiftExpression
    If rval is not an Object, throw a TypeError exception.
---*/

function makeClass() {
  return class {
    #x = 1;
    #m() { return this.#x; }
    get #acc() { return this.#x; }
    set #acc(v) { this.#x = v; }
    static has(o) { return #x in o; }
    static read(o) { return o.#x; }
    static write(o, v) { o.#x = v; }
    static callM(o) { return o.#m(); }
    static hasM(o) { return #m in o; }
    static bump(o) { return ++o.#acc; }
    static postBump(o) { return o.#acc++; }
    static add(o, v) { return o.#x += v; }
  };
}

for (var round = 0; round < 3; round++) {
  var A = makeClass();
  var B = makeClass();
  var a = new A();
  var b = new B();

  assert.sameValue(A.has(a), true, "own instance has #x");
  assert.sameValue(A.has(b), false, "another evaluation's instance lacks #x");
  assert.sameValue(B.has(a), false, "and vice versa");
  assert.sameValue(A.hasM(a), true, "#m in own instance");
  assert.sameValue(A.hasM(b), false, "#m in another evaluation's instance");

  assert.sameValue(A.read(a), 1);
  assert.throws(TypeError, function () { A.read(b); }, "read across evaluations");
  assert.throws(TypeError, function () { A.write(b, 2); }, "write across evaluations");
  assert.throws(TypeError, function () { A.callM(b); }, "method call across evaluations");
  assert.throws(TypeError, function () { A.bump(b); }, "update across evaluations");
  assert.throws(TypeError, function () { A.add(b, 1); }, "compound assignment across evaluations");

  A.write(a, 10);
  assert.sameValue(A.read(a), 10);
  assert.sameValue(A.callM(a), 10, "private method sees the receiver");
  assert.sameValue(A.bump(a), 11, "prefix update through an accessor");
  assert.sameValue(A.postBump(a), 11, "postfix update returns the old value");
  assert.sameValue(A.read(a), 12);
  assert.sameValue(A.add(a, 5), 17);
  assert.sameValue(B.read(b), 1, "the other evaluation is untouched");
}

assert.throws(TypeError, function () { A.has(1); }, "#x in non-object");
assert.throws(TypeError, function () { A.has(undefined); }, "#x in undefined");
assert.throws(TypeError, function () { A.read(null); }, "read from null");
assert.throws(TypeError, function () { A.write(undefined, 1); }, "write to undefined");

// Two classes spelling the same name do not alias.
class P { #v = "p"; static get(o) { return o.#v; } }
class Q { #v = "q"; static get(o) { return o.#v; } }
assert.sameValue(P.get(new P()), "p");
assert.sameValue(Q.get(new Q()), "q");
assert.throws(TypeError, function () { P.get(new Q()); }, "P's #v is not Q's #v");

// A nested class that does not redeclare a name sees the outer one; one that
// does shadows it.
class Outer {
  #a = "outer-a";
  #b = "outer-b";
  static make() {
    return class Inner {
      #b = "inner-b";
      static a(o) { return o.#a; }
      static b(o) { return o.#b; }
      static hasA(o) { return #a in o; }
      viaArrow(o) { return (() => o.#b)(); }
    };
  }
}
var Inner = Outer.make();
var outerInstance = new Outer();
var innerInstance = new Inner();
assert.sameValue(Inner.a(outerInstance), "outer-a", "outer name visible in nested class");
assert.sameValue(Inner.b(innerInstance), "inner-b", "nested class's own #b");
assert.throws(TypeError, function () { Inner.b(outerInstance); }, "shadowed #b is the inner one");
assert.sameValue(Inner.hasA(outerInstance), true);
assert.sameValue(Inner.hasA(innerInstance), false);
assert.sameValue(innerInstance.viaArrow(innerInstance), "inner-b", "arrow inside a method");

// A class evaluated inside a function: each call yields independent names.
function factory() {
  class C {
    #n = 0;
    static inc(o) { return ++o.#n; }
    static is(o) { return #n in o; }
  }
  return C;
}
var C1 = factory();
var C2 = factory();
assert.sameValue(C1.inc(new C1()), 1);
assert.sameValue(C1.is(new C2()), false);
assert.throws(TypeError, function () { C1.inc(new C2()); });

// Compound assignment reads the element before it evaluates the right-hand side.
var log = [];
class Order {
  #v = 1;
  get #g() { log.push("get"); return 5; }
  set #g(x) { log.push("set " + x); }
  static run(o, rhs) {
    o.#v += rhs();
    o.#g += rhs();
  }
  static view(o) { return o.#v; }
}
var order = new Order();
Order.run(order, function () { log.push("rhs"); return 2; });
assert.sameValue(Order.view(order), 3);
assert.sameValue(log.join(), "rhs,get,rhs,set 7");

// The right-hand side can change the element, but lval was already read.
class Race {
  #v = 1;
  static run(o) { return o.#v += (o.#v = 100, 1); }
  static view(o) { return o.#v; }
}
var race = new Race();
assert.sameValue(Race.run(race), 2, "lval is read before the RHS runs");
assert.sameValue(Race.view(race), 2);

// A simple assignment evaluates the RHS before PrivateSet reports a missing name.
var evaluated = false;
class Missing {
  #v;
  static set(o, f) { o.#v = f(); }
}
assert.throws(TypeError, function () {
  Missing.set({}, function () { evaluated = true; return 1; });
});
assert.sameValue(evaluated, true, "RHS evaluated before the PrivateSet TypeError");

// Methods are not writable; accessors need the matching half.
class Kinds {
  #m() {}
  get #ro() { return 1; }
  set #wo(v) {}
  static writeMethod(o) { o.#m = 1; }
  static writeGetterOnly(o) { o.#ro = 1; }
  static readSetterOnly(o) { return o.#wo; }
}
var kinds = new Kinds();
assert.throws(TypeError, function () { Kinds.writeMethod(kinds); });
assert.throws(TypeError, function () { Kinds.writeGetterOnly(kinds); });
assert.throws(TypeError, function () { Kinds.readSetterOnly(kinds); });

// Static private names.
class S {
  static #count = 0;
  static #bump() { return ++S.#count; }
  static next() { return S.#bump(); }
  static has(o) { return #count in o; }
}
assert.sameValue(S.next(), 1);
assert.sameValue(S.next(), 2);
assert.sameValue(S.has(S), true);
assert.sameValue(S.has({}), false);
