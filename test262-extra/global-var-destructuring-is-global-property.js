/*---
description: >
  Global var declarations bound by a destructuring pattern, with or without
  defaults, are properties of the global object, in scripts, in global eval code
  and in for-in/of heads, and their initializers go through [[Set]].
info: |
  GlobalDeclarationInstantiation (sec-globaldeclarationinstantiation) binds each
  declared var name with CreateGlobalVarBinding, which creates a property of the
  global object; a binding initialization of a var calls PutValue
  (sec-putvalue) on the resolved reference, so there is no declarative copy.
esid: sec-globaldeclarationinstantiation
flags: [noStrict]
---*/

var { sh1346 } = { sh1346: 5 };
var { kv1346: renamed1346 } = { kv1346: 6 };
var { dflt1346 = 7 } = {};
var [ a1346, b1346 = 8 ] = [ 1 ];
var [ c1346 = 9 ] = [];

assert.sameValue(globalThis.sh1346, 5, "shorthand");
assert.sameValue(globalThis.renamed1346, 6, "key-value");
assert.sameValue(globalThis.dflt1346, 7, "shorthand default");
assert.sameValue(globalThis.a1346, 1, "array element");
assert.sameValue(globalThis.b1346, 8, "array element default");
assert.sameValue(globalThis.c1346, 9, "array default");

for (var [ w1346 = 2 ] of [[]]) {}
assert.sameValue(globalThis.w1346, 2, "for-of head");
for (var { q1346 } of [{ q1346: 3 }]) {}
assert.sameValue(globalThis.q1346, 3, "for-of head, object pattern");

(0, eval)("var { ev1346 } = { ev1346: 4 }; var [ ee1346 = 5 ] = [];");
assert.sameValue(globalThis.ev1346, 4, "global eval");
assert.sameValue(globalThis.ee1346, 5, "global eval default");

var log = [];
Object.defineProperty(globalThis, "acc1346", {
  get: function () { return "getter"; },
  set: function (v) { log.push(v); },
  configurable: true
});
(0, eval)("var { acc1346 } = { acc1346: 'via pattern' };");
assert.sameValue(log.join(), "via pattern", "pattern initializer calls the setter");
assert.sameValue(acc1346, "getter");

globalThis.sh1346 = "replaced";
assert.sameValue(sh1346, "replaced", "bare read sees the property");
assert.sameValue(delete globalThis.dflt1346, false, "var properties are not configurable");
