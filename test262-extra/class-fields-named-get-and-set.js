// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-class-definitions-static-semantics-early-errors
description: >
  `get` and `set` start an accessor only when a property name follows; before
  `;`, `=`, `}` or a line break and `*` they are the names of fields.
features: [class, class-fields-public, class-static-fields-public]
includes: [propertyHelper.js]
---*/

class A {
  get;
  set = 1;
  static get = 2;
  static set;
  get
  *gen() { return 3; }
}

var a = new A();
assert(Object.prototype.hasOwnProperty.call(a, "get"), "instance field get");
assert.sameValue(a.get, undefined, "get field without initializer");
assert.sameValue(a.set, 1, "set field with initializer");
assert.sameValue(A.get, 2, "static get field");
assert(Object.prototype.hasOwnProperty.call(A, "set"), "static set field");
assert.sameValue(A.set, undefined, "static set field without initializer");
assert.sameValue(a.gen().next().value, 3, "generator after a get field");
verifyProperty(a, "set", { value: 1, writable: true, enumerable: true, configurable: true });

class B {
  get
}
assert(Object.prototype.hasOwnProperty.call(new B(), "get"), "get before }");

class C {
  set
}
assert(Object.prototype.hasOwnProperty.call(new C(), "set"), "set before }");

class D {
  get x() { return 4; }
  set x(v) { this.y = v; }
  get() { return 5; }
  set(v) { return v; }
}
var d = new D();
d.x = 6;
assert.sameValue(d.x, 4, "accessor pair still parsed");
assert.sameValue(d.y, 6, "setter ran");
assert.sameValue(d.get(), 5, "method named get");
assert.sameValue(d.set(7), 7, "method named set");

assert.throws(SyntaxError, function() {
  eval("class E { get = arguments; }");
}, "arguments in a get field initializer");
assert.throws(SyntaxError, function() {
  eval("class F { static set = arguments; }");
}, "arguments in a set field initializer");
