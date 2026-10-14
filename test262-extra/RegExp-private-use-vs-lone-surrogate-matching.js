// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-pattern-semantics
description: >
  Matching is over code points in Unicode mode and over code units otherwise.
  A genuine code point from U+F0000-U+10FFFF and a lone surrogate are different
  characters in every pattern construct, subject and result: literal atoms,
  escapes, classes, ranges, negation, property escapes, set notation, string
  alternatives, backreferences, lookbehind, and the exec, test, match,
  replace, search and split methods.
info: |
  22.2.2.7 Runtime Semantics: Canonicalize ( rer, ch )
  22.2.2.9 CharacterSetMatcher ( rer, A, invert, direction )
    ... each character of the input is a code unit, or a code point with the
    "u" or "v" flag.
features: [regexp-v-flag, regexp-lookbehind, regexp-named-groups]
---*/

function units(s) {
  var out = [];
  for (var i = 0; i < s.length; i++) out.push(s.charCodeAt(i).toString(16));
  return out.join(",");
}

var C0 = String.fromCodePoint(0xF0000);
var C1 = String.fromCodePoint(0xF07FF);
var C2 = String.fromCodePoint(0xF0800);
var CZ = String.fromCodePoint(0x10FFFF);
var LEAD = "\ud800";
var TRAIL = "\udc00";

function at(index, text) {
  return index + ":" + units(text);
}

function check(source, flags, subject, expected, label) {
  var re = new RegExp(source, flags);
  var m = re.exec(subject);
  var actual = m === null ? null : m.index + ":" + units(m[0]);
  assert.sameValue(actual, expected, label || (source + "/" + flags + " on " + units(subject)));
}

// Literal atoms and escapes.
check(C0, "u", C0, at(0, C0));
check(C0, "u", LEAD, null);
check(C0, "", C0, at(0, C0));
check(C0, "", LEAD, null);
check("\\ud800", "u", C0, null);
check("\\ud800", "u", LEAD, at(0, LEAD));
check("\\ud800", "", C0, null);
check("\\u{F0000}", "u", C0, at(0, C0));
check("\\u{F0000}", "u", LEAD, null);
check("\\udb80\\udc00", "u", C0, at(0, C0));
check("\\udb80\\udc00", "u", LEAD + TRAIL, null);
check("\\udbff\\udfff", "u", CZ, at(0, CZ));
check(C0 + "|\\ud800", "u", LEAD, at(0, LEAD));
check(C0 + "|\\ud800", "u", C0, at(0, C0));
check("\\ud800" + C1, "u", LEAD + C1, at(0, LEAD + C1));
check("\\ud800" + C1, "u", C0 + C1, null);

// Dot and negated classes take either.
check("^.$", "u", C0, at(0, C0));
check("^.$", "u", LEAD, at(0, LEAD));
check("^[^a]$", "u", C0, at(0, C0));
check("^[^a]$", "u", LEAD, at(0, LEAD));
check("^[^\\ud800]$", "u", C0, at(0, C0));
check("^[^\\ud800]$", "u", LEAD, null);
check("^[^" + C0 + "]$", "u", LEAD, at(0, LEAD));
check("^[^" + C0 + "]$", "u", C0, null);
check("^[^" + C0 + "]$", "u", C1, at(0, C1));

// Classes and ranges.
check("[\\ud800-\\udfff]", "u", C0, null);
check("[\\ud800-\\udfff]", "u", LEAD, at(0, LEAD));
check("[\\ud800-\\udfff]", "u", TRAIL, at(0, TRAIL));
check("[\\ud800-\\udbff]", "u", TRAIL, null);
check("[\\u{F0000}-\\u{F07FF}]", "u", C0, at(0, C0));
check("[\\u{F0000}-\\u{F07FF}]", "u", C1, at(0, C1));
check("[\\u{F0000}-\\u{F07FF}]", "u", LEAD, null);
check("[\\u{F0000}-\\u{F07FF}]", "u", TRAIL, null);
check("[\\u{F0000}-\\u{F07FF}]", "u", C2, null);
check("[\\u{EFFFF}-\\u{F1000}]", "u", C0, at(0, C0));
check("[\\u{EFFFF}-\\u{F1000}]", "u", LEAD, null);
check("[\\u{F0800}-\\u{10FFFF}]", "u", LEAD, null);
check("[\\u{F0800}-\\u{10FFFF}]", "u", C0, null);
check("[\\u{F0800}-\\u{10FFFF}]", "u", C2, at(0, C0).replace("db80,dc00", units(C2)));
check("[\\u0000-\\uffff]", "u", LEAD, at(0, LEAD));
check("[\\u0000-\\uffff]", "u", C0, null);
check("[\\u{10000}-\\u{10FFFF}]", "u", C0, at(0, C0));
check("[\\u{10000}-\\u{10FFFF}]", "u", LEAD, null);
check("[\\ud800" + C0 + "]", "u", C0, at(0, C0));
check("[\\ud800" + C0 + "]", "u", LEAD, at(0, LEAD));
check("[\\ud800" + C0 + "]", "u", C1, null);

// A subject holding both, in every position.
var both = C0 + LEAD + C1 + TRAIL + CZ;
check("\\ud800", "u", both, at(2, LEAD));
check("\\udc00", "u", both, at(5, TRAIL));
check(C1, "u", both, at(3, C1));
check(CZ, "u", both, at(6, CZ));
check("[\\ud800-\\udfff]+", "u", both, at(2, LEAD));
check("[\\ud800-\\udfff]", "gu", both, at(2, LEAD));
check("[^\\ud800-\\udfff]+", "u", both, at(0, C0));
check("(" + C0 + ")(\\ud800)(" + C1 + ")", "u", both, at(0, C0 + LEAD + C1));

// Property escapes.
check("\\p{Co}", "u", C0, at(0, C0));
check("\\p{Co}", "u", LEAD, null);
check("\\p{Cs}", "u", C0, null);
check("\\p{Cs}", "u", LEAD, at(0, LEAD));
check("\\P{Cs}", "u", C0, at(0, C0));
check("\\P{Cs}", "u", LEAD, null);
check("\\p{Any}", "u", LEAD, at(0, LEAD));
check("\\p{Any}", "u", C0, at(0, C0));
check("^\\P{Lu}$", "u", C0, at(0, C0));
check("^\\P{Lu}$", "u", LEAD, at(0, LEAD));
check("^\\p{Lu}$", "u", LEAD, null);

// Backreferences, lookbehind and named groups.
check("(\\ud800)\\1", "u", LEAD + LEAD, at(0, LEAD + LEAD));
check("(\\ud800)\\1", "u", LEAD + C0, null);
check("(" + C0 + ")\\1", "u", C0 + C0, at(0, C0 + C0));
check("(" + C0 + ")\\1", "u", C0 + LEAD, null);
check("(?<a>" + C0 + ")\\k<a>", "u", C0 + C0, at(0, C0 + C0));
check("(?<=" + C0 + ")x", "u", C0 + "x", at(2, "x"));
check("(?<=" + C0 + ")x", "u", LEAD + "x", null);
check("(?<=\\ud800)x", "u", LEAD + "x", at(1, "x"));
check("(?<=\\ud800)x", "u", C0 + "x", null);
check("(?<!\\ud800)x", "u", C0 + "x", at(2, "x"));
check("(?<!" + C0 + ")x", "u", C0 + "x", null);

// Set notation and string alternatives.
check("[\\ud800]", "v", LEAD, at(0, LEAD));
check("[\\ud800]", "v", C0, null);
check("[" + C0 + "]", "v", LEAD, null);
check("[^" + C0 + "]", "v", LEAD, at(0, LEAD));
check("[\\ud800-\\udfff]", "v", C0, null);
check("[\\ud800-\\udfff]", "v", LEAD, at(0, LEAD));
check("[\\q{\\ud800|ab}]", "v", "ab", at(0, "ab"));
check("[\\q{\\ud800|ab}]", "v", LEAD, at(0, LEAD));
check("[\\q{\\ud800|ab}]", "v", C0, null);
check("[\\q{" + C0 + "|ab}]", "v", C0, at(0, C0));
check("[\\q{" + C0 + "|ab}]", "v", LEAD, null);
check("[\\p{Cs}--\\ud800]", "v", LEAD, null);
check("[\\p{Cs}--\\ud800]", "v", TRAIL, at(0, TRAIL));
check("[\\p{Co}--" + C0 + "]", "v", C0, null);
check("[\\p{Co}--" + C0 + "]", "v", C1, at(0, C1));

// Methods built on the matcher report the original code units.
var s = "a" + C0 + "b" + LEAD + "c" + C0 + TRAIL;
assert.sameValue(s.replace(new RegExp(C0, "gu"), "_"), "a_b" + LEAD + "c_" + TRAIL, "replace genuine");
assert.sameValue(s.replace(/\ud800/gu, "_"), "a" + C0 + "b_c" + C0 + TRAIL, "replace lone");
assert.sameValue(s.replace(/[\ud800-\udfff]/gu, "_"), "a" + C0 + "b_c" + C0 + "_", "replace lone class");
assert.sameValue(s.search(new RegExp(C0, "u")), 1, "search genuine");
assert.sameValue(s.search(/\ud800/u), 4, "search lone");
assert.sameValue(units(s.split(new RegExp(C0, "u")).join("|")), units("a|b" + LEAD + "c|" + TRAIL), "split");
assert.sameValue(s.match(new RegExp(C0, "gu")).length, 2, "match genuine");
assert.sameValue(s.match(/[\ud800-\udfff]/gu).length, 2, "match lone");
assert.sameValue(Array.from(s.matchAll(/\ud800/gu)).length, 1, "matchAll lone");
var sticky = new RegExp(C0, "uy");
sticky.lastIndex = 1;
assert.sameValue(sticky.test(s), true, "sticky genuine at 1");
assert.sameValue(sticky.lastIndex, 3, "sticky lastIndex after genuine");
sticky.lastIndex = 3;
assert.sameValue(sticky.test(s), false, "sticky genuine at 3 (lone)");
var rep = "x".replace(/x/, "$&" + C0);
assert.sameValue(rep, "x" + C0, "replacement keeps genuine U+F0000");

// The source of a RegExp built from either is exact.
assert.sameValue(units(new RegExp(C0).source), units(C0), "source of genuine U+F0000");
assert.sameValue(units(new RegExp("\\ud800").source), units("\\ud800"), "source of escape");
assert.sameValue(units(new RegExp(LEAD, "u").source), units(LEAD), "source of lone surrogate");
assert.sameValue(units(RegExp.escape(C0)), units(C0), "RegExp.escape keeps genuine U+F0000");
assert.sameValue(units(RegExp.escape(LEAD)), units("\\ud800"), "RegExp.escape escapes a lone surrogate");
