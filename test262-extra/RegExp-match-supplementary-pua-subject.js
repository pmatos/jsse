/*---
esid: sec-regexpbuiltinexec
description: >
  RegExp result text preserves genuine supplementary Private Use Area code
  points in the subject.
info: |
  RegExpBuiltinExec ( R, S ) and RegExp.prototype [ %Symbol.split% ]

  Split substrings are slices of the original String S. A genuine U+F0000 is
  therefore the two code units 0xDB80 0xDC00, even though jsse's internal
  RegExp view uses U+F0000 as the sentinel for a lone 0xD800 surrogate.

  Subjects are built with String.fromCharCode so this test exercises only the
  RegExp conversion and result paths.
---*/

function assertSameUnits(actual, expected, label) {
  if (actual.length !== expected.length) {
    throw new Test262Error(
      label + ": expected " + expected.length + " code units, got " +
      actual.length
    );
  }
  for (var i = 0; i < expected.length; i++) {
    if (actual.charCodeAt(i) !== expected.charCodeAt(i)) {
      throw new Test262Error(
        label + ": code unit " + i + " should be " +
        expected.charCodeAt(i) + ", got " + actual.charCodeAt(i)
      );
    }
  }
}

var pua = String.fromCharCode(0xDB80, 0xDC00);

assertSameUnits(pua.split(/x/)[0], pua, "non-Unicode split");
assertSameUnits(pua.split(/x/u)[0], pua, "Unicode split");

var matched = /(?<plane>.)/du.exec(pua);
assertSameUnits(matched[0], pua, "exec full match");
assertSameUnits(matched[1], pua, "exec capture");
assertSameUnits(matched.groups.plane, pua, "exec named capture");
if (matched.index !== 0 || matched.indices[0][0] !== 0 || matched.indices[0][1] !== 2) {
  throw new Test262Error("exec indices must span UTF-16 offsets [0, 2]");
}

assertSameUnits(RegExp.lastMatch, pua, "RegExp.lastMatch");
assertSameUnits(RegExp.lastParen, pua, "RegExp.lastParen");
assertSameUnits(RegExp.$1, pua, "RegExp.$1");

var globalMatches = pua.match(/./gu);
if (globalMatches.length !== 1) {
  throw new Test262Error("global Unicode match should return one match");
}
assertSameUnits(globalMatches[0], pua, "global Unicode match text");

var iteratorResult = pua.matchAll(/./gu).next();
if (iteratorResult.done) {
  throw new Test262Error("matchAll should produce a match");
}
assertSameUnits(iteratorResult.value[0], pua, "matchAll text");

var sticky = /./uy;
var stickyMatch = sticky.exec(pua);
assertSameUnits(stickyMatch[0], pua, "sticky match text");
if (sticky.lastIndex !== 2) {
  throw new Test262Error("sticky lastIndex should be 2, got " + sticky.lastIndex);
}

// Controls: lone surrogates remain one code unit and Annex B statics preserve
// them, while an unrelated astral scalar remains its original surrogate pair.
var lone = String.fromCharCode(0xD800);
var loneMatch = /(.)/u.exec(lone);
assertSameUnits(loneMatch[0], lone, "lone surrogate match");
assertSameUnits(RegExp.lastMatch, lone, "lone surrogate RegExp.lastMatch");
assertSameUnits(RegExp.lastParen, lone, "lone surrogate RegExp.lastParen");
assertSameUnits(RegExp.$1, lone, "lone surrogate RegExp.$1");

var emoji = String.fromCharCode(0xD83D, 0xDE00);
assertSameUnits(/./u.exec(emoji)[0], emoji, "ordinary astral match");
