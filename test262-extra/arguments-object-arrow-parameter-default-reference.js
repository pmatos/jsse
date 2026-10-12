// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-functiondeclarationinstantiation
description: >
  An arrow function's parameter defaults evaluate in the enclosing function's
  scope chain, so a reference to `arguments` inside them (directly, through a
  nested direct eval, or inside a destructuring default) resolves to the
  enclosing function's arguments object, not to an unbound name.
info: |
  10.2.11 FunctionDeclarationInstantiation
    22. If argumentsObjectNeeded is true, then ... (arrow functions have no
        [[ThisMode]] lexical `arguments`; the identifier resolves through the
        outer Environment Record)
flags: [noStrict]
---*/

function direct() {
  return ((x = arguments) => x)();
}
var r = direct(10, 20, 30);
assert.sameValue(typeof r, "object", "direct: arguments object returned");
assert.sameValue(r.length, 3, "direct: length");
assert.sameValue(r[0], 10, "direct: first element");
assert.sameValue(r[2], 30, "direct: last element");

function viaEval() {
  return ((x = eval("arguments")) => x)();
}
var r2 = viaEval(7, 8);
assert.sameValue(typeof r2, "object", "eval: arguments object returned");
assert.sameValue(r2.length, 2, "eval: length");
assert.sameValue(r2[1], 8, "eval: second element");

function destructuring() {
  return (({ a = arguments[0] }) => a)({});
}
assert.sameValue(destructuring(42), 42, "destructuring default");

function arrayDestructuring() {
  return (([a = arguments.length]) => a)([]);
}
assert.sameValue(arrayDestructuring(1, 2, 3, 4), 4, "array pattern default");

function nestedArrows() {
  return (() => ((x = arguments[1]) => x)())();
}
assert.sameValue(nestedArrows("a", "b"), "b", "nested arrows");

function computedKey() {
  return (({ [arguments[0]]: v }) => v)({ k: "found" });
}
assert.sameValue(computedKey("k"), "found", "computed key in pattern");
