// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-performeval
description: >
  Source text given to eval, indirect eval, $262.evalScript and the Function
  constructors is a sequence of code units. A code point from the private-use
  range U+F0000-U+F0800 is read as itself in string literals, template literals
  (cooked and raw) and function source text, whether or not the same text also
  holds a lone surrogate, and a lone surrogate is read as itself in the same
  places.
info: |
  19.2.1.1 PerformEval ( x, strictCaller, direct )
    6. Let script be ParseText(x, Script).
  11.1 Source Text
    Source text is a sequence of Unicode code points; the string value of a
    string literal is the sequence of code units of its code points, a lone
    surrogate being its own code unit.
features: [template, String.raw]
---*/

function units(s) {
  var out = [];
  for (var i = 0; i < s.length; i++) out.push(s.charCodeAt(i));
  return out.join(",");
}

var P = String.fromCodePoint(0xF0000);
var P2 = String.fromCodePoint(0xF07FF);
var P3 = String.fromCodePoint(0xF0800);
var P4 = String.fromCodePoint(0xF0801);
var L = "\ud800";
var H = "\udc00";

var indirect = eval;
var evaluators = {
  "direct eval": function (src) { return eval(src); },
  "indirect eval": function (src) { return indirect(src); },
  "Function": function (src) { return new Function("return " + src)(); },
  "evalScript": function (src) { return $262.evalScript(src); }
};

Object.keys(evaluators).forEach(function (name) {
  var run = evaluators[name];

  assert.sameValue(run("'" + P + "'"), P, name + ": genuine U+F0000 alone");
  assert.sameValue(run("'" + P2 + "'"), P2, name + ": genuine U+F07FF alone");
  assert.sameValue(run("'" + P3 + "'"), P3, name + ": genuine U+F0800 alone");
  assert.sameValue(run("'" + P4 + "'"), P4, name + ": genuine U+F0801 alone");
  assert.sameValue(run("'" + L + "'"), L, name + ": lone surrogate alone");

  var mixed = P + L + P2 + H + P3 + P;
  assert.sameValue(run("'" + mixed + "'"), mixed, name + ": genuine and lone together");
  assert.sameValue(units(run("'" + mixed + "'")), units(mixed), name + ": mixed code units");

  assert.sameValue(run('"' + P + L + '"'), P + L, name + ": double-quoted");
  assert.sameValue(run("`" + P + L + P3 + "`"), P + L + P3, name + ": template cooked");
  assert.sameValue(run("'\\" + P + L + "'"), P + L, name + ": identity escape of a genuine code point");
  assert.sameValue(run("'" + P + L + "\\u{F0000}\\ud800'"), P + L + P + L, name + ": followed by escapes");
  assert.sameValue(run("'" + P3 + P3 + L + "'"), P3 + P3 + L, name + ": repeated U+F0800");
  assert.sameValue(run("'" + L + "\\\n" + P + "'"), L + P, name + ": after a line continuation");
  assert.sameValue(
    run("(function () { return '" + P + L + "'; })()"), P + L, name + ": inside a function");
  assert.sameValue(run("/* " + P + L + " */ '" + P + "'"), P, name + ": comment");
  assert.sameValue(run("'" + P3 + "'.length"), 2, name + ": length of genuine U+F0800");
});

assert.sameValue(
  eval("String.raw`" + P + L + P3 + "`"), P + L + P3, "template raw with genuine and lone");
assert.sameValue(eval("String.raw`" + L + "`"), L, "template raw with a lone surrogate");
assert.sameValue(eval("String.raw`" + P + "`"), P, "template raw with a genuine code point");
assert.sameValue(
  eval("String.raw`\\" + P + L + "`"), "\\" + P + L, "template raw after a backslash");
assert.sameValue(
  eval("String.raw`\\x" + P + L + "`"), "\\x" + P + L, "template raw after an invalid escape");
assert.sameValue(
  new Function("return String.raw`" + P + L + P3 + "`")(), P + L + P3, "Function template raw");
assert.sameValue(
  eval("(function (s) { return s.raw[0]; })`" + P + L + "`"), P + L, "tagged template raw[0]");

var f = eval("(function () { /* " + L + P + P3 + " */ return '" + P + L + "'; })");
assert.sameValue(
  f.toString(), "function () { /* " + L + P + P3 + " */ return '" + P + L + "'; }",
  "eval function toString");
assert.sameValue(f(), P + L, "eval function result");

var g = new Function("/* " + P + L + " */ return '" + P3 + L + "'");
assert.sameValue(
  g.toString(), "function anonymous(\n) {\n/* " + P + L + " */ return '" + P3 + L + "'\n}",
  "Function toString");
assert.sameValue(g(), P3 + L, "Function result");

assert.sameValue(
  new Function("a = '" + P + "'", "return a + '" + L + "'")(), P + L,
  "genuine in the parameters, lone surrogate in the body");
assert.sameValue(
  new Function("a = '" + L + "'", "return a + '" + P + "'")(), L + P,
  "lone surrogate in the parameters, genuine in the body");
assert.sameValue(
  new Function("a = '" + P3 + "'", "return a + '" + P + "'")(), P3 + P,
  "genuine only, in parameters and body");

assert.sameValue(eval("'" + P + "' + '" + P3 + "'"), P + P3, "no lone surrogate, two strings");
assert.sameValue(
  eval("var s = '" + P + "'; s + '" + L + "'"), P + L, "variable then lone surrogate");

assert.sameValue(
  eval("'\\u{F0000}' + '" + P + "'") , P + P, "escape and literal agree");
assert.sameValue(eval("'" + P + L + "'.codePointAt(0)"), 0xF0000, "codePointAt of genuine");
assert.sameValue(eval("'" + P + L + "'.codePointAt(2)"), 0xD800, "codePointAt of lone");
