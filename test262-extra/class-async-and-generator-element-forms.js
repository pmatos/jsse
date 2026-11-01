// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-class-definitions
description: async and * introduce a method only when a name, parameters and a body follow
info: |
  AsyncMethod : async [no LineTerminator here] ClassElementName ( UniqueFormalParameters ) { AsyncFunctionBody }
  FieldDefinition : ClassElementName Initializer_opt

  A line terminator after `async` ends the field named `async` by
  automatic semicolon insertion (12.10.1), and `async` followed by
  `;`, `=` or `}` is a field of that name.
---*/

var C = class {
  async
  x() {}
  async
  y
  async *g() {}
  static async m() {}
  *gen() {}
  async = 1;
};

var c = new C();
assert.sameValue(c.async, 1, "field named async");
assert.sameValue(c.hasOwnProperty("y"), true, "field y after a line terminator");
assert.sameValue(typeof C.prototype.x, "function", "method x after a line terminator");
assert.sameValue(Object.getPrototypeOf(C.prototype.g).constructor.name, "AsyncGeneratorFunction");
assert.sameValue(Object.getPrototypeOf(C.m).constructor.name, "AsyncFunction");

var D = class { async };
assert.sameValue(new D().hasOwnProperty("async"), true, "async before }");

var E = class { async; static async = 2; };
assert.sameValue(E.async, 2, "static field named async");

var F = class { async() { return 3; } };
assert.sameValue(new F().async(), 3, "method named async");

assert.throws(SyntaxError, function() {
  eval("(class { async x })");
}, "async x");
assert.throws(SyntaxError, function() {
  eval("(class { *x })");
}, "*x");
assert.throws(SyntaxError, function() {
  eval("(class { async *x })");
}, "async *x");
assert.throws(SyntaxError, function() {
  eval("(class { async x = 1 })");
}, "async x = 1");
