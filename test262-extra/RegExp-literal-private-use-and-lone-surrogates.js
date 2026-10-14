// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-regular-expression-literals-runtime-semantics-evaluation
description: >
  A regular expression literal denotes the code units of its pattern text. A
  code point from U+F0000-U+F07FF written in a literal, in a plain source file
  and in eval, direct or indirect, $262.evalScript and the Function
  constructors, is that code point and never a lone surrogate, and a lone
  surrogate written as an escape in the same literal stays a lone surrogate.
info: |
  13.2.7.3 Runtime Semantics: Evaluation
    RegularExpressionLiteral : /RegularExpressionLiteralChars/RegularExpressionFlags
      1. Let pattern be CodePointsToString(BodyText of RegularExpressionLiteral).
      2. Let flags be CodePointsToString(FlagText of RegularExpressionLiteral).
      3. Return ! RegExpCreate(pattern, flags).
  22.2.6.12 get RegExp.prototype.source
    ... the value of the [[OriginalSource]] internal slot, escaped.
---*/

function units(s) {
  var out = [];
  for (var i = 0; i < s.length; i++) out.push(s.charCodeAt(i));
  return out.join(",");
}

var P = String.fromCodePoint(0xF0000);
var P2 = String.fromCodePoint(0xF07FF);
var P3 = String.fromCodePoint(0xF0800);
var LONE = "\ud800";

// A literal in this source file.
var plain = /󰀀/;
assert.sameValue(units(plain.source), units(P), "plain literal source holds U+F0000");
assert.sameValue(plain.test(P), true, "plain literal matches U+F0000");
assert.sameValue(plain.test(LONE), false, "plain literal does not match a lone lead surrogate");
assert.sameValue(/󰟿/u.test(P2), true, "plain /u literal matches U+F07FF");
assert.sameValue(/󰟿/u.test(LONE), false, "plain /u literal does not match a lone surrogate");
assert.sameValue(/󰠀/u.test(P3), true, "plain /u literal matches U+F0800");

var indirect = eval;
var evaluators = {
  "direct eval": function (src) { return eval(src); },
  "indirect eval": function (src) { return indirect(src); },
  "Function": function (src) { return new Function("return " + src)(); },
  "evalScript": function (src) { return $262.evalScript(src); }
};

Object.keys(evaluators).forEach(function (name) {
  var run = evaluators[name];

  [0xF0000, 0xF0123, 0xF07FF, 0xF0800, 0xF0801, 0x10FFFF].forEach(function (cp) {
    var ch = String.fromCodePoint(cp);
    var label = name + " U+" + cp.toString(16).toUpperCase();
    var re = run("/" + ch + "/");
    assert.sameValue(units(re.source), units(ch), label + ": source");
    assert.sameValue(re.test(ch), true, label + ": matches itself");
    assert.sameValue(re.test("\ud800"), false, label + ": does not match a lone lead surrogate");
    assert.sameValue(re.test("\udc00"), false, label + ": does not match a lone trail surrogate");

    var withLone = run("/" + ch + "\\ud800/u");
    assert.sameValue(units(withLone.source), units(ch) + "," + 92 + ",117,100,56,48,48", label + ": source with escape");
    assert.sameValue(withLone.test(ch + "\ud800"), true, label + ": genuine then lone");
    assert.sameValue(withLone.test("\ud800" + ch), false, label + ": lone then genuine");

    var cls = run("/[" + ch + "]/u");
    assert.sameValue(cls.test(ch), true, label + ": class matches itself");
    assert.sameValue(cls.test("\ud800"), false, label + ": class does not match a lone surrogate");
  });

  var loneLiteral = run("/\ud800/");
  assert.sameValue(loneLiteral.test("\ud800"), true, name + ": escaped lone surrogate matches itself");
  assert.sameValue(loneLiteral.test(P), false, name + ": escaped lone surrogate is not U+F0000");

  var mixed = run("/" + P + "\ud800" + P2 + "/");
  assert.sameValue(mixed.test(P + "\ud800" + P2), true, name + ": genuine, lone, genuine");
  assert.sameValue(mixed.test("\ud800" + "\ud800" + P2), false, name + ": lone for genuine");
});
