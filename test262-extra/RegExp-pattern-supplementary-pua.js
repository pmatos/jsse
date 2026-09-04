/*---
esid: sec-regexpinitialize
description: >
  RegExp patterns preserve genuine supplementary Private Use Area code points
  in Unicode and non-Unicode modes.
info: |
  RegExpInitialize ( obj, pattern, flags ) and ParsePattern ( patternText,
  u, v )

  Without u or v, the pattern is interpreted as UTF-16 code units. With u or
  v, a surrogate pair is interpreted as one code point. Both modes must match
  the same genuine U+F0000-U+F07FF scalar in a subject, rather than confusing
  it with jsse's internal lone-surrogate sentinel.
---*/

function assertMatches(pattern, subject, flags, label) {
  var re = new RegExp(pattern, flags);
  if (!re.test(subject)) {
    throw new Test262Error(label + ": pattern did not match its source string");
  }
}

var base = String.fromCharCode(0xDB80, 0xDC00); // U+F0000
var top = String.fromCharCode(0xDB81, 0xDFFF);  // U+F07FF

assertMatches(base, base, "", "U+F0000 non-Unicode");
assertMatches(base, base, "u", "U+F0000 Unicode");
assertMatches(top, top, "", "U+F07FF non-Unicode");
assertMatches(top, top, "u", "U+F07FF Unicode");

// Controls for the values on either side of the internal representation:
// lone surrogates and unrelated supplementary scalars keep matching.
var lone = String.fromCharCode(0xD800);
var emoji = String.fromCharCode(0xD83D, 0xDE00);
assertMatches(lone, lone, "", "lone surrogate non-Unicode");
assertMatches(lone, lone, "u", "lone surrogate Unicode");
assertMatches(emoji, emoji, "", "ordinary astral non-Unicode");
assertMatches(emoji, emoji, "u", "ordinary astral Unicode");
