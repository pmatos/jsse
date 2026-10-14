// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-tostring
description: >
  ToString results that reach JS keep lone surrogates: Symbol description,
  Symbol.for keys, Error message (all native errors), Error.prototype.toString,
  escape and unescape.
features: [Symbol, Symbol.prototype.description]
---*/

var L = "\ud800";
var H = "\ud801";

assert.sameValue(Symbol(L).description, L, "Symbol(description)");

assert.sameValue(Symbol.for(L).description, L, "Symbol.for description");
assert.sameValue(Symbol.keyFor(Symbol.for(L)), L, "Symbol.keyFor");
assert.notSameValue(Symbol.for(L), Symbol.for(H), "distinct lone-surrogate registry keys");
assert.sameValue(Symbol.for(L), Symbol.for(L), "same lone-surrogate registry key");
assert.sameValue(Symbol.for(L), Symbol.for("\ud800"), "registry lookup by value");

assert.sameValue(new Error(L).message, L, "Error message");
assert.sameValue(new TypeError(L).message, L, "TypeError message");
assert.sameValue(new AggregateError([], L).message, L, "AggregateError message");
assert.sameValue(new SuppressedError(undefined, undefined, L).message, L, "SuppressedError message");

assert.sameValue(
  Error.prototype.toString.call({ name: L, message: "" }), L, "toString name only");
assert.sameValue(
  Error.prototype.toString.call({ name: "", message: L }), L, "toString message only");
assert.sameValue(
  Error.prototype.toString.call({ name: "N", message: L }), "N: " + L, "toString both");
assert.sameValue(
  Error.prototype.toString.call({ name: L, message: H }), L + ": " + H, "toString both lone");

assert.sameValue(escape(L), "%uD800", "escape");
assert.sameValue(escape("a" + L + "b"), "a%uD800b", "escape mixed");
assert.sameValue(unescape("%uD800"), L, "unescape %u");
assert.sameValue(unescape(L), L, "unescape passes lone surrogate through");
assert.sameValue(unescape("a" + H + "%41"), "a" + H + "A", "unescape mixed");
