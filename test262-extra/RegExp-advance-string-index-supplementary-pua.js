/*---
esid: sec-advancestringindex
description: >
  AdvanceStringIndex advances empty matches by the original UTF-16 code units,
  so a genuine supplementary Private Use Area scalar in U+F0000-U+F07FF is
  never mistaken for jsse's one-code-unit lone-surrogate sentinel.
info: |
  AdvanceStringIndex ( S, index, unicode )

  3. If unicode is false, return index + 1.
  4. If index + 1 >= the length of S, return index + 1.
  5. Let cp be CodePointAt(S, index).
  6. Return index + cp.[[CodeUnitCount]].

  The spec defines this over the original String S. jsse matches against a
  converted Rust view in which a *lone* surrogate is encoded as a PUA scalar in
  U+F0000-U+F07FF, an encoding that is ambiguous with a *genuine* scalar in that
  same range. Deriving the advance from that view makes a real U+F0000 look like
  a one-code-unit sentinel, so a global Unicode empty match advances to UTF-16
  index 1, the boundary map sends index 1 back to the start of the scalar, and
  the same empty match repeats forever. AdvanceStringIndex must therefore read
  the retained UTF-16 code units, where a high/low surrogate pair is
  unambiguously two units wide.

  Every expectation below is cross-checked against V8. The failure mode this
  guards is a hang, so a regression shows up as a timeout rather than a
  mismatch.

  Subjects are built with String.fromCharCode rather than source literals so the
  test exercises the RegExp conversion path only.
---*/

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

function assertIndices(actual, expected, label) {
  if (actual.length !== expected.length) {
    throw new Test262Error(
      label + ": expected " + expected.length + " matches, got " +
      actual.length + " [" + actual.join(",") + "]"
    );
  }
  for (var i = 0; i < expected.length; i++) {
    if (actual[i] !== expected[i]) {
      throw new Test262Error(
        label + ": match " + i + " should be at index " + expected[i] +
        ", got " + actual[i]
      );
    }
  }
}

function matchAllIndices(s, re) {
  var indices = [];
  var iter = s.matchAll(re);
  var step = iter.next();
  while (!step.done) {
    indices.push(step.value.index);
    step = iter.next();
  }
  return indices;
}

// U+F0000, the base of jsse's lone-surrogate PUA sentinel range, as a genuine
// two-code-unit supplementary scalar.
var pua = String.fromCharCode(0xDB80, 0xDC00);
var lone = String.fromCharCode(0xD800);
var emoji = String.fromCharCode(0xD83D, 0xDE00);

if (pua.length !== 2 || pua.codePointAt(0) !== 0xF0000) {
  throw new Test262Error("subject construction failed: length " + pua.length);
}

// The core repro. In Unicode mode the scalar is two code units wide, so an
// empty match at 0 must advance past both of them and the scan must terminate
// with exactly two matches.
assertSameUnits(pua.replace(/(?:)/gu, "Z"), "Z" + pua + "Z", "unicode empty replace");
assertIndices(matchAllIndices(pua, /(?:)/gu), [0, 2], "unicode empty matchAll");

var m = pua.match(/(?:)/gu);
if (m.length !== 2) {
  throw new Test262Error("unicode empty match should yield 2 matches, got " + m.length);
}

// Without the u flag the same subject is two independent code units, so
// AdvanceStringIndex steps one unit at a time and there are three positions.
assertSameUnits(
  pua.replace(/(?:)/g, "Z"),
  "Z" + String.fromCharCode(0xDB80) + "Z" + String.fromCharCode(0xDC00) + "Z",
  "non-unicode empty replace"
);
assertIndices(matchAllIndices(pua, /(?:)/g), [0, 1, 2], "non-unicode empty matchAll");

// Control: what the sentinel range actually encodes. A lone surrogate is one
// code unit wide even in Unicode mode, so it advances by one.
assertSameUnits(lone.replace(/(?:)/gu, "Z"), "Z" + lone + "Z", "lone surrogate empty replace");
assertIndices(matchAllIndices(lone, /(?:)/gu), [0, 1], "lone surrogate empty matchAll");

// Control: an ordinary astral scalar outside the sentinel range behaves the
// same as the PUA one, confirming the advance is driven by the surrogate pair
// and not by the code point's value.
assertSameUnits(emoji.replace(/(?:)/gu, "Z"), "Z" + emoji + "Z", "astral empty replace");
assertIndices(matchAllIndices(emoji, /(?:)/gu), [0, 2], "astral empty matchAll");
assertSameUnits(
  emoji.replace(/(?:)/g, "Z"),
  "Z" + String.fromCharCode(0xD83D) + "Z" + String.fromCharCode(0xDE00) + "Z",
  "astral non-unicode empty replace"
);

// A subject holding both a real sentinel and a genuine PUA scalar: the original
// code units disambiguate them within one scan.
var mixed = lone + pua + "x";
assertSameUnits(
  mixed.replace(/(?:)/gu, "Z"),
  "Z" + lone + "Z" + pua + "Z" + "x" + "Z",
  "mixed empty replace"
);
assertIndices(matchAllIndices(mixed, /(?:)/gu), [0, 1, 3, 4], "mixed empty matchAll");

// The scalar surrounded by BMP characters, so the advance is exercised from a
// non-zero starting index rather than only at position 0.
var padded = "a" + pua + "b";
assertSameUnits(
  padded.replace(/(?:)/gu, "Z"),
  "Z" + "a" + "Z" + pua + "Z" + "b" + "Z",
  "padded empty replace"
);
assertIndices(matchAllIndices(padded, /(?:)/gu), [0, 1, 3, 4], "padded empty matchAll");

// Split drives the same advance through its own loop, so the scalar must stay
// whole rather than being torn at the sentinel boundary.
if (padded.split(/(?:)/gu).length !== 3) {
  throw new Test262Error(
    "unicode empty split should yield 3 elements, got " +
    padded.split(/(?:)/gu).length
  );
}

// A quantified pattern that can match empty must terminate too: the advance
// only runs when the match is zero-length, and the scalar still counts as two
// units when it does.
assertSameUnits(pua.replace(/x*/gu, "Z"), "Z" + pua + "Z", "nullable quantifier replace");
assertIndices(matchAllIndices(pua, /x*/gu), [0, 2], "nullable quantifier matchAll");

// Driving exec directly with an explicit lastIndex covers the remaining
// boundary edges. A lastIndex landing inside the surrogate pair is rounded down
// to the start of the scalar in Unicode mode, so the match reports index 0 --
// the pair is never entered at its trailing half.
var re = /(?:)/gu;
re.lastIndex = 1;
var mid = re.exec(pua);
if (mid === null || mid.index !== 0) {
  throw new Test262Error(
    "exec from mid-pair lastIndex should match at index 0, got " +
    (mid === null ? "null" : mid.index)
  );
}

re.lastIndex = 2;
var end = re.exec(pua);
if (end === null || end.index !== 2) {
  throw new Test262Error(
    "exec from end lastIndex should match at index 2, got " +
    (end === null ? "null" : end.index)
  );
}

re.lastIndex = 3;
if (re.exec(pua) !== null) {
  throw new Test262Error("exec past the end should return null");
}
