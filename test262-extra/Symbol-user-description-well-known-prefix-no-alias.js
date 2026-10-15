// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-ecmascript-language-types-symbol-type
description: >
  A Symbol created with Symbol() or Symbol.for() whose description starts with
  "Symbol." is a distinct value from the well-known symbol of that name and
  from every other Symbol; the description is not an identity.
info: |
  6.1.5 The Symbol Type
    Each Symbol value immutably holds an associated value called [[Description]]
    ... Each Symbol is unique and immutable.
  6.1.5.1 Well-Known Symbols
  20.4.1.1 Symbol ( [ description ] )
    4. Return a new Symbol whose [[Description]] is descString.
  20.4.2.2 Symbol.for ( key )
includes: [compareArray.js]
---*/

var a = Symbol("Symbol.iterator");
var o = {};
o[a] = 1;
assert.sameValue(o[Symbol.iterator], undefined, "user symbol does not define @@iterator");
assert.sameValue(Object.getOwnPropertySymbols(o)[0], a, "own key is the user symbol");
assert.notSameValue(Object.getOwnPropertySymbols(o)[0], Symbol.iterator, "own key is not @@iterator");
assert.sameValue(o[a], 1);

var b1 = Symbol("Symbol.x");
var b2 = Symbol("Symbol.x");
var q = {};
q[b1] = 1;
q[b2] = 2;
assert.sameValue(Object.getOwnPropertySymbols(q).length, 2, "equal descriptions stay distinct keys");
assert.sameValue(q[b1], 1);
assert.sameValue(q[b2], 2);
assert.compareArray(Reflect.ownKeys(q), [b1, b2], "Reflect.ownKeys returns the user symbols");

var c = Symbol("Symbol.toStringTag");
var t = {};
t[c] = "X";
assert.sameValue(Object.prototype.toString.call(t), "[object Object]", "@@toStringTag unaffected");
assert.sameValue(Object.getOwnPropertySymbols(t)[0], c);
assert.notSameValue(Object.getOwnPropertySymbols(t)[0], Symbol.toStringTag);

var it = {};
it[a] = function () {
  return { next: function () { return { done: true }; } };
};
assert.throws(TypeError, function () {
  for (var x of it) {}
}, "an object keyed by a user symbol is not iterable");

var f = Symbol.for("Symbol.iterator");
var p = {};
p[f] = 1;
assert.sameValue(p[Symbol.iterator], undefined, "Symbol.for key does not define @@iterator");
assert.sameValue(Object.getOwnPropertySymbols(p)[0], f);
assert.notSameValue(f, Symbol.iterator);
assert.sameValue(Symbol.for("Symbol.iterator"), f, "registry symbol stays stable");
assert.sameValue(Symbol.keyFor(f), "Symbol.iterator");
assert.sameValue(Symbol.keyFor(Symbol.iterator), undefined);

var wk = {};
wk[Symbol.iterator] = 7;
assert.sameValue(Object.getOwnPropertySymbols(wk)[0], Symbol.iterator, "real well-known key is unchanged");
assert.sameValue(wk[Symbol.iterator], 7);
assert.sameValue(wk[Symbol("Symbol.iterator")], undefined, "fresh user symbol does not read @@iterator");
assert.sameValue(Symbol.iterator.description, "Symbol.iterator");
assert.sameValue(a.description, "Symbol.iterator");
