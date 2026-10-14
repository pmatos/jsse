// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-createdynamicfunction
description: >
  The parameter and body Strings given to the Function, GeneratorFunction,
  AsyncFunction and AsyncGeneratorFunction constructors are source text whose
  code units are kept, so a lone surrogate in a literal evaluates to itself
  and appears in the function's toString output.
flags: [async]
includes: [asyncHelpers.js]
features: [generators, async-functions, async-iteration]
---*/

var L = "\ud800";
var H = "\udc00";

var GeneratorFunction = Object.getPrototypeOf(function*() {}).constructor;
var AsyncFunction = Object.getPrototypeOf(async function() {}).constructor;
var AsyncGeneratorFunction = Object.getPrototypeOf(async function*() {}).constructor;

assert.sameValue(new Function("return '" + L + "'")(), L, "Function string literal");
assert.sameValue(new Function("return \"a" + H + "b\"")(), "a" + H + "b", "Function low surrogate");
assert.sameValue(new Function("a = '" + L + "'", "b = '" + H + "'", "return a + b")(), L + H,
  "Function parameter defaults");
assert.sameValue(new Function("return `x" + L + "y`")(), "x" + L + "y", "Function template literal");
assert.sameValue(new Function("/* " + L + " */ return 1")(), 1, "lone surrogate in a comment");
assert.sameValue(new Function("return '😀'")(), "😀", "surrogate pair");

assert.sameValue(GeneratorFunction("yield '" + L + "'")().next().value, L, "GeneratorFunction");
var g = GeneratorFunction("a = '" + H + "'", "yield a");
assert.sameValue(g().next().value, H, "GeneratorFunction parameter default");

var p = AsyncFunction("return '" + L + "'")();
var q = AsyncGeneratorFunction("yield '" + H + "'")().next();

assert.sameValue(
  new Function("return '" + L + "'").toString(),
  "function anonymous(\n) {\nreturn '" + L + "'\n}",
  "Function toString");
assert.sameValue(
  GeneratorFunction("yield '" + L + "'").toString(),
  "function* anonymous(\n) {\nyield '" + L + "'\n}",
  "GeneratorFunction toString");
assert.sameValue(
  AsyncFunction("return '" + L + "'").toString(),
  "async function anonymous(\n) {\nreturn '" + L + "'\n}",
  "AsyncFunction toString");
assert.sameValue(
  AsyncGeneratorFunction("yield '" + L + "'").toString(),
  "async function* anonymous(\n) {\nyield '" + L + "'\n}",
  "AsyncGeneratorFunction toString");

assert.sameValue(new Function("return '\u{F0000}'")(), "\u{F0000}", "private-use plane 15 code point");
assert.sameValue(
  new Function("return '\u{F0000}'").toString(),
  "function anonymous(\n) {\nreturn '\u{F0000}'\n}",
  "private-use code point in toString");

var nested = new Function("return function() { return '" + L + "'; }")();
assert.sameValue(nested(), L, "nested function result");
assert.sameValue(nested.toString(), "function() { return '" + L + "'; }", "nested function toString");

var Cls = new Function("return class C { m() { return '" + L + "'; } }")();
assert.sameValue(Cls.toString(), "class C { m() { return '" + L + "'; } }", "nested class toString");
assert.sameValue(new Cls().m.toString(), "m() { return '" + L + "'; }", "nested method toString");

assert.sameValue(new Function("return String.raw`a" + L + "\\n`")(), "a" + L + "\\n", "String.raw");
var rawStrings = new Function("tag", "return tag`x" + H + "y`")(function(strings) { return strings.raw[0]; });
assert.sameValue(rawStrings, "x" + H + "y", "template raw string");

assert.throws(SyntaxError, function() {
  new Function("var " + L);
}, "lone surrogate is not an identifier character");

asyncTest(function() {
  return p.then(function(v) {
    assert.sameValue(v, L, "AsyncFunction result");
    return q;
  }).then(function(r) {
    assert.sameValue(r.value, H, "AsyncGeneratorFunction result");
  });
});
