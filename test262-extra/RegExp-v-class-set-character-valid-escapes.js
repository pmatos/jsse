// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-patterns-static-semantics-early-errors
description: >
  With the "v" flag, the escapes that ClassSetCharacter allows (a
  CharacterEscape in Unicode mode, `\b`, and a ClassSetReservedPunctuator) are
  accepted in a character class and as ClassSetRange endpoints, a surrogate
  pair written as two "\u" escapes is one endpoint, and the escapes denote the
  expected characters.
info: |
  22.2.1 Patterns
    ClassSetCharacter ::
      [lookahead ∉ ClassSetReservedDoublePunctuator] SourceCharacter but not ClassSetSyntaxCharacter
      `\` CharacterEscape[+UnicodeMode]
      `\` ClassSetReservedPunctuator
      `\` `b`
    ClassSetReservedPunctuator :: one of & - ! # % , : ; < = > @ ` ~
    RegExpUnicodeEscapeSequence[UnicodeMode] ::
      `u` HexLeadSurrogate `\u` HexTrailSurrogate
      `u{` CodePoint `}`
features: [regexp-v-flag]
---*/

function test(source, yes, no) {
  var re = new RegExp(source, "v");
  for (var i = 0; i < yes.length; i++) {
    assert.sameValue(re.test(yes[i]), true, source + " matches " + yes[i]);
  }
  for (var j = 0; j < no.length; j++) {
    assert.sameValue(re.test(no[j]), false, source + " does not match " + no[j]);
  }
}

test("^[\\&\\-\\!\\#\\%\\,\\:\;\\<\\=\\>\\@\\`\\~]$",
  ["&", "-", "!", "#", "%", ",", ":", ";", "<", "=", ">", "@", "`", "~"], ["a", "\\"]);
test("^[\\^\\$\\\\\\.\\*\\+\\?\\(\\)\\[\\]\\{\\}\\|\\/]$",
  ["^", "$", "\\", ".", "*", "+", "?", "(", ")", "[", "]", "{", "}", "|", "/"], ["a"]);
test("^[\\b]$", ["\b"], ["b"]);
test("^[\\f\\n\\r\\t\\v\\0]$", ["\f", "\n", "\r", "\t", "\v", "\0"], ["f", "n"]);
test("^[\\cA-\\cZ]$", ["\x01", "\x1a"], ["\x00", "\x1b", "A"]);
test("^[\\x41-\\x5a]$", ["A", "Z"], ["@", "["]);
test("^[\\u0061-\\u007A]$", ["a", "z"], ["`", "{"]);
test("^[\\u{61}-\\u{7a}]$", ["a", "z"], ["`", "{"]);
test("^[\\u{0000000061}]$", ["a"], ["b"]);
test("^[\\u{1F600}-\\u{1F64F}]$", ["\u{1F600}", "\u{1F64F}"], ["\u{1F5FF}", "\u{1F650}"]);
test("^[\\uD83D\\uDE00-\\uD83D\\uDE01]$", ["\u{1F600}", "\u{1F601}"], ["\u{1F602}", "\uD83D"]);
test("^[\\uD83D]$", ["\uD83D"], ["\u{1F600}"]);
test("^[\\-\\--\\-]$", ["-"], ["a"]);
test("^[a-a\\d\\p{Lu}\\q{xy|z}]$", ["a", "5", "Q", "xy", "z"], ["b", "x", "y"]);
test("^[\\d--\\q{5}]$", ["4"], ["5"]);
test("^[\\w&&[a-c]]$", ["a", "c"], ["d"]);
