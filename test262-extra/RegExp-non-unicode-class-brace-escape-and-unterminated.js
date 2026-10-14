// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-patterns-static-semantics-early-errors
description: >
  Without the "u" and "v" flags, "\u{" in a character class is the identity
  escape "u" followed by the literal characters "{", the hex digits and "}"
  (Annex B ClassAtomNoDash / IdentityEscape), so "\u{1}-\u{2}" is a class
  range "}-u" that is out of order and "a-\u{2}" is the range "a-u". A
  character class with no closing "]" is a SyntaxError in every mode.
info: |
  B.1.2 Regular Expressions Patterns
    ClassAtomNoDash[UnicodeMode, NamedCaptureGroups] ::
      `\` ClassEscape[?UnicodeMode, ?NamedCaptureGroups]
    ClassEscape[UnicodeMode] :: ...  [~UnicodeMode] CharacterEscape
    CharacterEscape :: ... IdentityEscape[~UnicodeMode]
  22.2.1.1 Static Semantics: Early Errors
    NonemptyClassRanges :: ClassAtom `-` ClassAtom ClassRanges
      It is a Syntax Error if IsCharacterClass of the first ClassAtom is
      false, IsCharacterClass of the second ClassAtom is false, and the
      CharacterValue of the first ClassAtom is larger than the second.
---*/

function matches(source, flags, subject) {
  return new RegExp(source, flags).test(subject);
}

var outOfOrder = [
  "[\\u{1}-\\u{2}]",
  "[\\u{1}-u]",
  "[\\u{1}-\\u{2}]+",
  "[\\u{}-\\u{2}]",
  "[\\u{1,2}-\\u{3}]",
  "[\\u{61}-\\u{62}]",
  "[\\u{2}-\\u{1}]"
];
for (var i = 0; i < outOfOrder.length; i++) {
  var source = outOfOrder[i];
  assert.throws(SyntaxError, function() { new RegExp(source); }, source);
  assert.throws(SyntaxError, function() { new RegExp(source, "i"); }, source + " /i");
  assert.throws(SyntaxError, function() { new RegExp(source, "g"); }, source + " /g");
  assert.throws(SyntaxError, function() { RegExp(source); }, "RegExp(" + source + ")");
  assert.throws(SyntaxError, function() { eval("/" + source + "/"); }, "literal " + source);
}

var matching = [
  ["[\\u{61}]", ["u", "{", "6", "1", "}"], ["a", "a", "\u0001"]],
  ["[a-\\u{2}]", ["a", "m", "u", "{", "2", "}"], ["b-", "1", "v", "\u0002"]],
  ["[u-\\u{1}]", ["u", "{", "1", "}"], ["t", "v", "\u0001"]],
  ["[\\u{1}-}]", ["u", "{", "1", "}"], ["-", "\u0001", "~"]],
  ["[\\u{1}]", ["u", "{", "1", "}"], ["\u0001"]],
  ["\\u{1}", ["u"], ["\u0001", "{", "1"]]
];
for (var j = 0; j < matching.length; j++) {
  var source = matching[j][0];
  var yes = matching[j][1];
  var no = matching[j][2];
  for (var k = 0; k < yes.length; k++) {
    assert.sameValue(matches(source, "", yes[k]), true, source + " matches " + yes[k]);
    assert.sameValue(matches(source, "i", yes[k]), true, source + "/i matches " + yes[k]);
  }
  for (var k = 0; k < no.length; k++) {
    var sub = no[k];
    if (sub.length === 1) {
      assert.sameValue(matches(source, "", sub), false, source + " does not match " + sub.charCodeAt(0));
    }
  }
}

assert.sameValue(/[\u{61}]/.exec("xa}")[0], "}", "the class does not contain a");

var unicodeAccepted = ["u", "v", "iu"];
for (var m = 0; m < unicodeAccepted.length; m++) {
  var f = unicodeAccepted[m];
  assert.sameValue(matches("[\\u{1}-\\u{2}]", f, "\u0001"), true, "/" + f + " \\u{1}");
  assert.sameValue(matches("[\\u{1}-\\u{2}]", f, "\u0002"), true, "/" + f + " \\u{2}");
  assert.sameValue(matches("[\\u{1}-\\u{2}]", f, "u"), false, "/" + f + " not u");
  assert.sameValue(matches("[\\u{1}-\\u{2}]", f, "}"), false, "/" + f + " not }");
  assert.sameValue(matches("[\\u{61}]", f, "a"), true, "/" + f + " \\u{61}");
  assert.sameValue(matches("[\\u{61}]", f, "u"), false, "/" + f + " \\u{61} not u");
}
assert.throws(SyntaxError, function() { new RegExp("[\\u{2}-\\u{1}]", "u"); }, "/u out of order");
assert.throws(SyntaxError, function() { new RegExp("[\\u{110000}]", "u"); }, "/u beyond U+10FFFF");
assert.throws(SyntaxError, function() { new RegExp("[\\u{1,2}]", "u"); }, "/u not hex");

var unterminated = ["[a", "[", "[^", "a[", "[\\u{", "[\\u{1", "[\\]", "[a-", "[\\u0041", "(?:[a)"];
var flagSets = ["", "i", "g", "m", "s", "y", "u", "v"];
for (var n = 0; n < unterminated.length; n++) {
  for (var p = 0; p < flagSets.length; p++) {
    var src = unterminated[n];
    var fl = flagSets[p];
    assert.throws(SyntaxError, function() { new RegExp(src, fl); }, "/" + src + "/" + fl);
  }
}

assert.sameValue(new RegExp("[]").test("a"), false, "empty class matches nothing");
assert.sameValue(new RegExp("[^]").test("a"), true, "negated empty class matches anything");
assert.sameValue(new RegExp("[\\]]").test("]"), true, "escaped ] inside a class");
assert.sameValue(new RegExp("a]").test("a]"), true, "a lone ] is a pattern character");

assert.sameValue(/[\u{0}-￿]/.test("\ud800"), true, "range from } to U+FFFF holds a lone surrogate");
assert.sameValue(/[\u{0}-￿]/.test("\udfff"), true, "range from } to U+FFFF holds a trailing surrogate");
assert.throws(SyntaxError, function() {
  eval("(function() { /[\\u{1}-\\u{2}]/; })");
}, "an out of order range is an early error in a literal");
assert.throws(SyntaxError, function() {
  eval("(function() { /[a/; })");
}, "an unterminated class is an early error in a literal");
