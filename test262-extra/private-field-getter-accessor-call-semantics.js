// Copyright (C) 2026 jsse contributors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
description: >
  An accessor whose getter only returns a private field (`get x() { return
  this.#x; }`) behaves exactly as an ordinary [[Call]] of the getter: it reads
  the element through the receiver, throws a TypeError for a receiver that
  lacks the element (another instance of a different class evaluation, an
  object that merely inherits from an instance, a primitive), reports the
  error in the getter's realm, keeps the lexical `this` of an arrow getter,
  and still bounds mutual recursion through private accessors.
esid: sec-ordinarycallbindthis
features: [class, class-fields-private, class-methods-private, cross-realm, Reflect]
info: |
  10.2.1 [[Call]] ( thisArgument, argumentsList )
    Perform OrdinaryCallBindThis; evaluate the function body.

  7.3.31 PrivateGet ( O, P )
    1. Let entry be PrivateElementFind(O, P).
    2. If entry is empty, throw a TypeError exception.
---*/

function makeClass() {
  return class {
    #x = 1;
    get x() { return this.#x; }
    set x(v) { this.#x = v; }
    static get sx() { return this.#sx; }
    static #sx = 7;
  };
}

var A = makeClass();
var B = makeClass();
var a = new A();
var b = new B();

for (var i = 0; i < 5; i++) {
  a.x = 1;
  assert.sameValue(a.x, 1, "own read " + i);
  a.x = i + 10;
  assert.sameValue(a.x, i + 10, "read after write " + i);
  assert.sameValue(A.sx, 7, "static getter " + i);
}

for (var i = 0; i < 5; i++) {
  assert.throws(TypeError, function() {
    Reflect.get(A.prototype, "x", b);
  }, "receiver from a different class evaluation " + i);
  assert.throws(TypeError, function() {
    Reflect.get(A.prototype, "x", Object.create(a));
  }, "receiver that only inherits from an instance " + i);
  assert.throws(TypeError, function() {
    Reflect.get(A.prototype, "x", 1);
  }, "primitive receiver " + i);
  assert.throws(TypeError, function() {
    Reflect.get(A, "sx", {});
  }, "static getter on a foreign receiver " + i);
}

var inheritor = Object.create(a);
assert.throws(TypeError, function() {
  inheritor.x;
}, "member access through an inheriting object");

var getter = Object.getOwnPropertyDescriptor(A.prototype, "x").get;
for (var i = 0; i < 5; i++) {
  assert.sameValue(getter.call(a), a.x, "getter.call on an instance " + i);
}
assert.throws(TypeError, function() {
  getter.call(b);
}, "getter.call on a foreign instance");

class Arrow {
  #x = "lexical";
  static make() {
    var o = { get x() { return "wrong"; } };
    Object.defineProperty(o, "y", { get: () => this.#x });
    return o;
  }
}
var arrowHolder = Arrow.make();
for (var i = 0; i < 5; i++) {
  assert.throws(TypeError, function() {
    arrowHolder.y;
  }, "arrow getter keeps the lexical `this`, which is the class, not an instance " + i);
}

class Chain {
  #a = 0;
  get #viaB() { return this.#viaA; }
  get #viaA() { return this.#viaB; }
  get a() { return this.#a; }
  get loop() { return this.#viaA; }
}
var chain = new Chain();
for (var i = 0; i < 3; i++) {
  assert.sameValue(chain.a, 0, "plain private field getter " + i);
}
assert.throws(RangeError, function() {
  chain.loop;
}, "mutually recursive private accessors overflow the call stack");

class Acc {
  get #inner() { return "inner"; }
  #m() { return "method"; }
  get viaAccessor() { return this.#inner; }
  get viaMethod() { return this.#m; }
}
var acc = new Acc();
for (var i = 0; i < 5; i++) {
  assert.sameValue(acc.viaAccessor, "inner", "getter returning a private accessor " + i);
  assert.sameValue(typeof acc.viaMethod, "function", "getter returning a private method " + i);
}

var other = $262.createRealm().global;
var OtherA = other.eval("(class { #x = 1; get x() { return this.#x; } })");
var otherInstance = new OtherA();
for (var i = 0; i < 5; i++) {
  assert.sameValue(otherInstance.x, 1, "cross-realm getter " + i);
  var thrown;
  try {
    Reflect.get(OtherA.prototype, "x", {});
  } catch (e) {
    thrown = e;
  }
  assert.sameValue(
    thrown.constructor,
    other.TypeError,
    "the brand-check error comes from the getter's realm " + i
  );
}
