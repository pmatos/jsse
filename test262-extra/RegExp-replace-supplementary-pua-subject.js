// Tests that RegExp replacement preserves subject code units that encode
// supplementary code points in the Private Use Area plane 15 range
// U+F0000-U+F07FF.
// Spec: ECMAScript 2026, sec-regexp.prototype-%symbol.replace% steps 14-16
//
// jsse converts a UTF-16 subject into a Rust String for matching, encoding
// lone surrogates as PUA scalars in U+F0000-U+F07FF. A *genuine* supplementary
// code point in that same range must not be confused with that encoding: the
// non-Unicode matching view has to be built from the original UTF-16 code
// units, not derived from the Unicode view, or a real U+F0000 collapses into a
// single lone surrogate and the unchanged portion of the subject is corrupted.
//
// Built with String.fromCharCode rather than a source literal so the test
// exercises the RegExp conversion path only.

function codeUnits(s) {
  var units = [];
  for (var i = 0; i < s.length; i++) {
    units.push(s.charCodeAt(i));
  }
  return units;
}

function assertSameUnits(actual, expected, label) {
  var a = codeUnits(actual);
  var e = codeUnits(expected);
  if (a.length !== e.length) {
    throw new Test262Error(
      label + ": expected " + e.length + " code units, got " + a.length +
      " [" + a.join(",") + "]"
    );
  }
  for (var i = 0; i < e.length; i++) {
    if (a[i] !== e[i]) {
      throw new Test262Error(
        label + ": code unit " + i + " should be " + e[i] + ", got " + a[i]
      );
    }
  }
}

// The surrogate pair encoding of U+F0000, the base of jsse's lone-surrogate
// PUA range.
var pua = String.fromCharCode(0xDB80, 0xDC00);

if (pua.length !== 2 || pua.codePointAt(0) !== 0xF0000) {
  throw new Test262Error("subject construction failed: length " + pua.length);
}

// A pattern that never matches must return the subject unchanged.
assertSameUnits(pua.replace(/x/, "y"), pua, "non-matching replace");
assertSameUnits(pua.replace(/x/g, "y"), pua, "non-matching global replace");
assertSameUnits(pua.replace(/x/u, "y"), pua, "non-matching unicode replace");
assertSameUnits(pua.replace(/x/gu, "y"), pua, "non-matching global unicode replace");

// A match elsewhere in the subject must leave the supplementary code point
// intact in both the leading and trailing unchanged portions.
var padded = "a" + pua + "b";
if (padded.length !== 4) {
  throw new Test262Error("padded subject should be 4 code units, got " + padded.length);
}

assertSameUnits(padded.replace(/a/, "Z"), "Z" + pua + "b", "leading replace");
assertSameUnits(padded.replace(/a/g, "Z"), "Z" + pua + "b", "leading global replace");
assertSameUnits(padded.replace(/b/, "Z"), "a" + pua + "Z", "trailing replace");
assertSameUnits(padded.replace(/b/g, "Z"), "a" + pua + "Z", "trailing global replace");
assertSameUnits(padded.replace(/a/gu, "Z"), "Z" + pua + "b", "leading global unicode replace");

// Functional replacement receives the intact subject as its final argument,
// and the match position is a UTF-16 offset into that subject.
var seenString = null;
var seenPosition = null;
var functional = padded.replace(/b/g, function (match, position, string) {
  seenPosition = position;
  seenString = string;
  return "Z";
});
assertSameUnits(functional, "a" + pua + "Z", "functional replace result");
assertSameUnits(seenString, padded, "functional replace subject argument");
if (seenPosition !== 3) {
  throw new Test262Error("functional replace position should be 3, got " + seenPosition);
}

// $` and $' substitutions slice the same intact subject.
assertSameUnits(
  padded.replace(/b/, "[$`]"),
  "a" + pua + "[a" + pua + "]",
  "$` substitution"
);
assertSameUnits(
  padded.replace(/a/, "[$']"),
  "[" + pua + "b]" + pua + "b",
  "$' substitution"
);

// A non-Unicode pattern sees the subject as two individual code units, so the
// match index of a following character reflects both of them.
var m = padded.match(/b/);
if (m.index !== 3) {
  throw new Test262Error("match index should be 3, got " + m.index);
}
assertSameUnits(padded.match(/./)[0], "a", "non-unicode single unit match");

// Offsets derived from the Unicode matching view must count such a scalar as its
// real two code units, not as the one-code-unit surrogate sentinel. This governs
// `.index`, the Unicode-mode replacement slice, and the Annex B lazy
// `leftContext`/`rightContext`, which retain UTF-16 offsets into the subject.
assertSameUnits(padded.replace(/b/u, "Z"), "a" + pua + "Z", "trailing unicode replace");
assertSameUnits(padded.replace(/b/gu, "Z"), "a" + pua + "Z", "trailing global unicode replace");

var mu = padded.match(/b/u);
if (mu.index !== 3) {
  throw new Test262Error("unicode match index should be 3, got " + mu.index);
}

/x/u.exec(pua + "x");
assertSameUnits(RegExp.leftContext, pua, "unicode leftContext");
/x/u.exec("x" + pua);
assertSameUnits(RegExp.rightContext, pua, "unicode rightContext");
/x/.exec(pua + "x");
assertSameUnits(RegExp.leftContext, pua, "non-unicode leftContext");
/x/.exec("x" + pua);
assertSameUnits(RegExp.rightContext, pua, "non-unicode rightContext");

// The sentinel range must keep working for what it actually encodes: a lone
// surrogate stays one code unit, and an ordinary astral character stays two.
var lone = String.fromCharCode(0xD800);
/x/.exec(lone + "x");
assertSameUnits(RegExp.leftContext, lone, "lone surrogate leftContext");

var emoji = String.fromCharCode(0xD83D, 0xDE00);
/x/u.exec(emoji + "x");
assertSameUnits(RegExp.leftContext, emoji, "astral unicode leftContext");
/x/.exec(emoji + "x");
assertSameUnits(RegExp.leftContext, emoji, "astral non-unicode leftContext");
assertSameUnits(emoji.replace(/x/g, "y"), emoji, "astral replace unchanged");
assertSameUnits(lone.replace(/x/g, "y"), lone, "lone surrogate replace unchanged");
