// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-arrow-function-definitions
description: an unparenthesized ArrowFunction cannot be continued by a binary, conditional or assignment operator
info: |
  AssignmentExpression[In, Yield, Await] :
    ConditionalExpression[?In, ?Yield, ?Await]
    ArrowFunction[?In, ?Yield, ?Await]
    ...

  ArrowFunction is not a ShortCircuitExpression, so no operator can follow it.
  Automatic semicolon insertion (12.9.1) does not repair these inputs: with no
  line terminator before the offending token no semicolon is inserted, and with
  one the token that follows the inserted semicolon cannot start a statement.
---*/

var sources = [
  "var f = () => { } / 2",
  "var f = () => { }\n/ 2",
  "var f = () => { }\n* 2",
  "var f = () => { } + 1",
  "var f = () => { } || 1",
  "var f = () => { } ? 1 : 2",
  "var f = () => { }\n? 1 : 2",
  "var f = () => { } instanceof Object",
  "var f = () => { }\nin {}",
  "var f = () => { } = 1",
  "var f = () => { } /= 1",
  "var f = () => { }\n.x",
  "var f = x => { } ** 2",
  "var f = async () => { } & 1",
  "var f = () => { (1) } / 2",
  "var f = () => { (a) }\n? 1 : 2",
  "var f = x => { return (x) }\n.x",
  "var f = (a = (1)) => { } / 2",
  "var f = () => { (1) } in {}",
  "class A extends () => { (1) } {}",
];

sources.forEach(function(src) {
  assert.throws(SyntaxError, function() {
    eval(src);
  }, src);
});

assert.sameValue(
  typeof eval("var g = () => { }\n+1\ng"),
  "function",
  "a unary '+' statement after the inserted semicolon is valid"
);
