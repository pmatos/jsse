// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-patterns-static-semantics-early-errors
description: >
  With the "v" flag, an escape in a character class must be a valid
  ClassSetCharacter or CharacterClassEscape, and a ClassSetRange needs two
  ClassSetCharacter endpoints in order; every other form is an early
  SyntaxError, both from the RegExp constructor and from a regular
  expression literal.
info: |
  22.2.1 Patterns
    ClassSetRange :: ClassSetCharacter `-` ClassSetCharacter
    ClassSetCharacter ::
      [lookahead ∉ ClassSetReservedDoublePunctuator] SourceCharacter but not ClassSetSyntaxCharacter
      `\` CharacterEscape[+UnicodeMode]
      `\` ClassSetReservedPunctuator
      `\` `b`
  22.2.1.1 Static Semantics: Early Errors
    ClassSetRange :: ClassSetCharacter `-` ClassSetCharacter
      It is a Syntax Error if the CharacterValue of the first ClassSetCharacter
      is larger than the CharacterValue of the second ClassSetCharacter.
    RegExpUnicodeEscapeSequence[UnicodeMode] :: `u{` CodePoint `}`
      It is a Syntax Error if the MV of HexDigits > 0x10FFFF.
features: [regexp-v-flag]
---*/

var invalid = [
  // identity escapes of non-syntax characters and other bad CharacterEscapes
  "[\\z]", "[\\a]", "[\\_]", "[\\ ]", "[\\k]", "[\\B]", "[\\8]", "[\\1]", "[\\00]",
  // malformed or out of range Unicode escapes
  "[\\u{110000}]", "[\\u{1,2}]", "[\\u{}]", "[\\u12]", "[\\u]", "[\\u{]", "[\\u{1]",
  "[\\u{0110000}]",
  // malformed hex and control escapes
  "[\\x1]", "[\\x]", "[\\c]", "[\\c1]",
  // malformed property and string-disjunction escapes
  "[\\p]", "[\\P]", "[\\p{L]", "[\\q]", "[\\q{a]", "[\\q{\\z}]", "[\\q{\\d}]", "[\\q{a-b}]",
  // out of order ranges
  "[\\u{2}-\\u{1}]", "[b-a]", "[\\x5a-\\x41]", "[\\cb-\\ca]", "[\\u{10FFFF}-\\u{10FFFE}]",
  "[\\uD83D\\uDE01-\\uD83D\\uDE00]", "[\\u{DFFF}-\\u{D800}]", "[a-\\u{110000}]",
  // class escapes, nested classes and string disjunctions as range endpoints
  "[a-\\d]", "[\\d-a]", "[a-\\p{L}]", "[\\p{L}-a]", "[\\w-\\d]", "[[a]-b]", "[a-[b]]",
  "[\\q{a}-b]", "[a-\\q{b}]",
  // a range end that is not a ClassSetCharacter
  "[a-]", "[a-!!]", "[a-z-9]"
];

for (var i = 0; i < invalid.length; i++) {
  var source = invalid[i];
  assert.throws(SyntaxError, function() { new RegExp(source, "v"); }, "RegExp(" + source + ", v)");
  assert.throws(SyntaxError, function() { new RegExp(source + "+", "v"); }, "RegExp(" + source + "+, v)");
  assert.throws(SyntaxError, function() { new RegExp("[" + source + "]", "v"); },
    "nested in RegExp([" + source + "], v)");
  assert.throws(SyntaxError, function() { eval("/" + source + "/v"); }, "/" + source + "/v");
}
