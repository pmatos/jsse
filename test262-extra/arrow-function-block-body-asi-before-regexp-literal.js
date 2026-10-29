// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-automatic-semicolon-insertion
description: a line terminator after a block-bodied arrow function inserts a semicolon, so a following "/" starts a RegularExpressionLiteral
info: |
  12.9.1 Rules of Automatic Semicolon Insertion

  When, as the source text is parsed from left to right, a token (called the
  offending token) is encountered that is not allowed by any production of the
  grammar, then a semicolon is automatically inserted before the offending
  token if the offending token is separated from the previous token by at
  least one LineTerminator.

  ArrowFunction is an AssignmentExpression, never an operand of "/", so "/"
  is an offending token there and the input element after the semicolon is
  InputElementRegExp (12.9.4).
---*/

const f = () => { }
/a/.test("a")

var g = x => { }
/a/g

var h = async () => { }
/=a/g

var count = 0;
var k = (a, b) => { count++; }
/b/.exec("b")
assert.sameValue(typeof f, "function");
assert.sameValue(typeof g, "function");
assert.sameValue(typeof h, "function");
assert.sameValue(typeof k, "function");
assert.sameValue(count, 0, "the arrow body must not run");

assert.sameValue(
  eval("var m = () => { }\n/x/.test('x')\n m;") instanceof Function,
  true,
  "regexp literal statement after arrow, then another statement"
);

assert.sameValue(
  eval("var n = () => { }\n/x/g\nvar o = 7; o"),
  7,
  "regexp literal statement ends at the next line"
);

assert.sameValue(
  eval("var p = () => { }\n/x/.source"),
  "x",
  "member access on the regexp literal"
);

assert.sameValue(
  eval("var q; q = () => { }\n/=x/.source"),
  "=x",
  "a regexp literal that starts with '='"
);

assert.sameValue(
  eval("var r = 8; var s = () => r\n/2/1; s()"),
  4,
  "an expression-bodied arrow keeps '/' as division"
);

assert.sameValue(
  eval("var r = 8; var t = () => r / 2; t()"),
  4,
  "division inside an expression body"
);

assert.sameValue(
  eval("var u = (() => { })\n/1/1; typeof u"),
  "number",
  "a parenthesized arrow is a primary expression: '/' is division"
);
