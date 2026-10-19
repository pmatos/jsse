// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-new-operator
description: >
  In `new new F()()` the inner `new F()` is a MemberExpression, so the second
  argument list and any member suffix belong to the outer `new`.
info: |
  13.3 Left-Hand-Side Expressions

  MemberExpression :
    MemberExpression . IdentifierName
    MemberExpression [ Expression ]
    new MemberExpression Arguments

  NewExpression :
    MemberExpression
    new NewExpression
features: [new.target]
---*/

var seen;

function F() {
  return function G(a, b) {
    this.k = 1;
    this.args = arguments.length;
    this.a = a;
    this.b = b;
  };
}

var r = new new F()();
assert.sameValue(r.k, 1, "new new F()() constructs the function F returns");
assert.sameValue(r.args, 0, "outer Arguments is the empty list");

r = new new F()(7, 8);
assert.sameValue(r.args, 2, "outer Arguments reach the outer new");
assert.sameValue(r.a, 7, "first outer argument");
assert.sameValue(r.b, 8, "second outer argument");

assert.sameValue(new new F()().k, 1, "member access on the outer new result");

function Inner() {
  seen = arguments.length;
  return function Made() { this.made = true; };
}

new new Inner(1, 2, 3)(4);
assert.sameValue(seen, 3, "inner Arguments reach the inner new only");

var ns = {
  Maker: function() { return function() { this.z = 3; }; }
};
assert.sameValue(new new ns.Maker()().z, 3, "inner callee with member access");

var trace = [];
function Holder() { return Holder; }
Holder.make = function() {
  trace.push("make:" + arguments.length);
  return function Product() { trace.push("product"); };
};
var made = new new Holder().make(1, 2);
assert.sameValue(typeof made, "function", "member suffix after the inner new applies before the outer new");
assert.sameValue(trace.join(), "make:2", "outer Arguments reach the constructed member");

var viaParens = new (new F())();
assert.sameValue(viaParens.k, 1, "parenthesized form agrees");

function NoArgs() { return function Last() { this.last = true; }; }
assert.sameValue(typeof new new NoArgs, "object", "new new F without any Arguments");

function NT(depth) {
  if (depth === 0) {
    return function Leaf() { this.leaf = true; };
  }
  return new new new.target(0)();
}
assert.sameValue(new NT(1).leaf, true, "new new new.target(0)() constructs what new.target(0) returns");
