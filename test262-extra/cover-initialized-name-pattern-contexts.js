// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-object-initializer-static-semantics-early-errors
description: CoverInitializedName stays valid wherever the literal is reinterpreted as a pattern
info: |
  ObjectLiteral : { PropertyDefinitionList }
  PropertyDefinition : CoverInitializedName

  The early error applies only when the literal is not refined to an
  AssignmentPattern (13.15.5.1), ArrowParameters (15.3.1) or a for-in/of
  head (14.7.5.1), so each of these contexts parses and destructures.
---*/

var a, b, log;

({ a = 1 } = {});
assert.sameValue(a, 1, "object assignment pattern");

[{ a = 2 }] = [{}];
assert.sameValue(a, 2, "object pattern inside an array pattern");

({ x: { a = 3 } } = { x: {} });
assert.sameValue(a, 3, "object pattern inside an object pattern");

[...[{ a = 4 }]] = [[{}]];
assert.sameValue(a, 4, "array pattern in a rest element");

[{ a = 5 } = {}] = [];
assert.sameValue(a, 5, "pattern with its own default");

({ a = 6 } = { b = 7 } = { b: 8 });
assert.sameValue(a, 6, "outer assignment pattern");
assert.sameValue(b, 8, "inner assignment pattern");

var f1 = ({ a = 9 }) => a;
assert.sameValue(f1({}), 9, "arrow parameter");

var f2 = ([{ a = 10 }], { b = 11 } = {}) => a + b;
assert.sameValue(f2([{}]), 21, "nested arrow parameters");

var f3 = async ({ a = 12 }, [{ b = 13 }]) => a + b;
assert.sameValue(typeof f3, "function", "async arrow parameters");

log = [];
for ({ a = 14 } of [{}, { a: 15 }]) log.push(a);
assert.sameValue(log.join(), "14,15", "for-of head");

log = [];
for ([{ a = 16 }] of [[{}]]) log.push(a);
assert.sameValue(log.join(), "16", "for-of head, nested pattern");

log = [];
for ({ a = 17 } in { k: 1 }) log.push(a);
assert.sameValue(log.join(), "17", "for-in head");

var x = [({ a = 18 } = {}), 0];
assert.sameValue(a, 18, "assignment inside a parenthesised element");

assert.throws(SyntaxError, function() {
  eval("({ a = 1 }) = {}");
}, "a parenthesised literal is not a pattern");
