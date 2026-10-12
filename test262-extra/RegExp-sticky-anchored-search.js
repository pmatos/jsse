/*---
esid: sec-regexpbuiltinexec
description: >
  A sticky match is attempted only at lastIndex: a failure resets lastIndex to
  0 even when the pattern would match later in the subject, and assertions
  (lookahead, lookbehind, anchors, word boundaries, backreferences) still see
  the whole subject rather than the suffix starting at lastIndex.
info: |
  RegExpBuiltinExec ( R, S )

  12. Repeat, while matchSucceeded is false,
    a. If lastIndex > length, then ... set lastIndex to 0 and return null.
    c. Let r be matcher(input, inputIndex).
    d. If r is failure, then
      i. If sticky is true, then
        1. Perform ? Set(R, "lastIndex", +0𝔽, true).
        2. Return null.
      ii. Set lastIndex to AdvanceStringIndex(S, lastIndex, fullUnicode).

  The engine may run the matcher anchored at lastIndex or search forward and
  reject a match that starts later; both are observably the same, and this test
  pins the observable behaviour of every pattern family the engine compiles
  through a different matcher.
includes: [compareArray.js]
---*/

function stickyAt(re, subject, index) {
  re.lastIndex = index;
  var m = re.exec(subject);
  return m === null ? null : m[0] + "@" + m.index + ">" + re.lastIndex;
}

// Plain matcher (no lookaround): the digit exists, but not at lastIndex.
var hashes = "#".repeat(40) + "5";
var digit = /\d/y;
assert.sameValue(stickyAt(digit, hashes, 3), null, "plain, fails before the digit");
assert.sameValue(digit.lastIndex, 0, "plain, lastIndex reset on failure");
assert.sameValue(stickyAt(digit, hashes, 40), "5@40>41", "plain, matches at lastIndex");

// Lookahead forces the backtracking matcher.
var notZero = /(?!0)\d/y;
assert.sameValue(stickyAt(notZero, hashes, 3), null, "lookahead, fails before the digit");
assert.sameValue(notZero.lastIndex, 0, "lookahead, lastIndex reset on failure");
assert.sameValue(stickyAt(notZero, hashes, 40), "5@40>41", "lookahead, matches at lastIndex");
assert.sameValue(stickyAt(notZero, "#0", 1), null, "lookahead rejects at lastIndex");

// Backreference inside a lookahead, as a tokenizer's string-literal rule.
var stringLiteral = /(['"])(?:(?!\1)[^\\\n\r]|\\[^])*\1/y;
var withString = "x = 'a\\'b' + \"c\";";
assert.sameValue(stickyAt(stringLiteral, withString, 0), null, "string literal, not at 0");
assert.sameValue(stickyAt(stringLiteral, withString, 4), "'a\\'b'@4>10", "string literal, escaped quote");
assert.sameValue(stickyAt(stringLiteral, withString, 13), "\"c\"@13>16", "string literal, double quote");
assert.sameValue(stickyAt(stringLiteral, withString, 5), null, "string literal, not at an opening quote");

// A literal prefix must also be located at lastIndex, not later.
var prefixed = /abc(?!d)/y;
assert.sameValue(stickyAt(prefixed, "xxabc", 0), null, "literal prefix later in subject");
assert.sameValue(stickyAt(prefixed, "xxabc", 2), "abc@2>5", "literal prefix at lastIndex");
assert.sameValue(stickyAt(prefixed, "xxabcd", 2), null, "literal prefix rejected by lookahead");

// ^ is the start of the subject, not of the sticky search.
assert.sameValue(stickyAt(/^x/y, "ax", 1), null, "^ is not satisfied at lastIndex 1");
assert.sameValue(stickyAt(/^x/y, "xa", 0), "x@0>1", "^ at lastIndex 0");
assert.sameValue(stickyAt(/^x/my, "a\nx", 2), "x@2>3", "^ after a line terminator, multiline");
assert.sameValue(stickyAt(/^x/my, "a\nx", 1), null, "multiline ^ not after a line terminator");

// Lookbehind and \b inspect the characters before lastIndex.
assert.sameValue(stickyAt(/(?<=a)b/y, "ab", 1), "b@1>2", "lookbehind sees the prefix");
assert.sameValue(stickyAt(/(?<=c)b/y, "ab", 1), null, "lookbehind fails on the prefix");
assert.sameValue(stickyAt(/(?<!a)b/y, "ab", 1), null, "negative lookbehind sees the prefix");
assert.sameValue(stickyAt(/\bb/y, "ab", 1), null, "\\b between two word characters");
assert.sameValue(stickyAt(/\bb/y, "a b", 2), "b@2>3", "\\b after a space");
assert.sameValue(stickyAt(/\Bb/y, "ab", 1), "b@1>2", "\\B between two word characters");

// The first alternative that succeeds at lastIndex wins, as in the unanchored search.
assert.sameValue(stickyAt(/a|ab/y, "xab", 1), "a@1>2", "leftmost alternative");
assert.sameValue(stickyAt(/ab|a/y, "xab", 1), "ab@1>3", "leftmost alternative, longer first");

// Empty matches are matches at lastIndex.
assert.sameValue(stickyAt(/(?:)/y, "abc", 2), "@2>2", "empty match at lastIndex");
assert.sameValue(stickyAt(/\d*/y, "ab1", 0), "@0>0", "star matches empty at lastIndex");
assert.sameValue(stickyAt(/\d*/y, "ab1", 2), "1@2>3", "star matches the digit at lastIndex");

// Captures and named groups come from the attempt at lastIndex.
var captured = /(?<word>[a-z]+)(\d)?/y;
captured.lastIndex = 2;
var m = captured.exec("12ab3");
assert.sameValue(m[0], "ab3");
assert.sameValue(m.index, 2);
assert.sameValue(m[1], "ab");
assert.sameValue(m[2], "3");
assert.sameValue(m.groups.word, "ab");
assert.sameValue(captured.lastIndex, 5);

// Unicode mode: lastIndex counts code units, matches are by code point.
var astral = "𝌆a";
assert.sameValue(stickyAt(/./uy, astral, 0), "𝌆@0>2", "u: whole surrogate pair");
assert.sameValue(stickyAt(/a/uy, astral, 2), "a@2>3", "u: after the pair");
assert.sameValue(stickyAt(/a/uy, astral, 0), null, "u: not at the pair");

// Lone surrogates take the WTF-8 matcher.
var lone = "x\uD800y";
assert.sameValue(stickyAt(/\p{Cs}/uy, lone, 0), null, "\\p{Cs} later in subject");
assert.sameValue(stickyAt(/\p{Cs}/uy, lone, 1), "\uD800@1>2", "\\p{Cs} at lastIndex");
assert.sameValue(lone.length, 3);

// Global + sticky loops stop at the first position that fails.
assert.compareArray("aaXaa".match(/a/gy), ["a", "a"], "match with gy");
assert.sameValue("aaXaa".replace(/a/gy, "b"), "bbXaa", "replace with gy");
assert.compareArray("a,b,c".split(/,/y), ["a", "b", "c"], "split with a sticky separator");
assert.compareArray("a1b22c".split(/(?!0)\d+/y), ["a", "b", "c"], "split, lookahead separator");

var gy = /a/gy;
gy.lastIndex = 1;
assert.sameValue("baa".replace(gy, "X"), "baa", "global replace restarts at 0, where sticky fails");
assert.sameValue(gy.lastIndex, 0, "lastIndex is 0 after the failed global replace");
