// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-patterns-static-semantics-early-errors
description: \b in a class is U+0008 as a range endpoint, and a class escape endpoint is invalid with u
info: |
  ClassEscape[UnicodeMode] :: b
  NonemptyClassRanges :: ClassAtom - ClassAtom ClassContents

  It is a Syntax Error if IsCharacterClass of the first ClassAtom is true or
  IsCharacterClass of the second ClassAtom is true and this production has a
  [UnicodeMode] parameter. It is a Syntax Error if IsCharacterClass of both
  atoms is false and the CharacterValue of the first is larger than that of
  the second. The CharacterValue of `\b` is 0x0008.
---*/

assert.throws(SyntaxError, function() { new RegExp("[\\b-\\d]", "u"); }, "\\b-\\d with u");
assert.throws(SyntaxError, function() { new RegExp("[\\b-\\w]", "u"); }, "\\b-\\w with u");
assert.throws(SyntaxError, function() { new RegExp("[\\b-\\D]", "u"); }, "\\b-\\D with u");
assert.throws(SyntaxError, function() { new RegExp("[\\d-\\b]", "u"); }, "\\d-\\b with u");
assert.throws(SyntaxError, function() { new RegExp("[\\b-\\d]", "v"); }, "\\b-\\d with v");
assert.throws(SyntaxError, function() { new RegExp("[\\b-\\x01]"); }, "\\b-\\x01 is out of order");
assert.throws(SyntaxError, function() { new RegExp("[\\b-\\x01]", "u"); }, "\\b-\\x01 is out of order with u");
assert.throws(SyntaxError, function() { new RegExp("[\\x09-\\b]"); }, "\\x09-\\b is out of order");

assert.sameValue(new RegExp("[\\b-\\d]").test("5"), true, "Annex B: \\d is an atom without u");
assert.sameValue(new RegExp("[\\b-\\d]").test("-"), true, "Annex B: - is an atom without u");
assert.sameValue(new RegExp("[\\b-\\d]").test("\b"), true, "Annex B: \\b is an atom without u");
assert.sameValue(new RegExp("[\\x01-\\b]", "u").test("\x05"), true, "range up to \\b");
assert.sameValue(new RegExp("[\\b-a]", "u").test("\x09"), true, "range from \\b");
assert.sameValue(new RegExp("[\\b-a]", "u").test("\x07"), false, "below \\b");
assert.sameValue(new RegExp("[\\b-\\b]", "u").test("\b"), true, "range \\b-\\b");
