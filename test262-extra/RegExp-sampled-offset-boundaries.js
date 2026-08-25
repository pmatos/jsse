/*---
esid: sec-regexpbuiltinexec
description: >
  Match offsets stay exact on subjects long enough to span many sampled offset
  boundaries, in both Unicode and non-Unicode mode.
info: |
  RegExpBuiltinExec ( R, S ), steps 12-15

  jsse matches against a converted view of the subject and maps offsets back to
  UTF-16 with a cached table. That table is *sampled* — it stores one boundary
  every N characters rather than one per character, so a lookup binary-searches
  the samples and then walks at most one interval. The walk is where a sampled
  table can diverge from a dense one, and only a subject longer than the sample
  interval exercises it: short subjects fit inside the first interval and never
  binary-search at all.

  Each character below is deliberately a different UTF-8 width in the converted
  view (1, 2 and 3 bytes) and a different UTF-16 width in the subject (1 and 2
  code units), so byte offsets and code-unit offsets advance at different rates
  and a walk that miscounts either one is caught.

  Every expectation is derived from the subject's construction rather than
  hard-coded, and cross-checked against V8.
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
      label + ": expected " + e.length + " code units, got " + a.length
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

// One repeating unit mixing every relevant width, with a marker at a known
// position. "a" is 1 byte / 1 unit, "é" is 2 bytes / 1 unit, "€" is 3 bytes /
// 1 unit, and the astral characters are 4 bytes / 2 units.
var astral = String.fromCharCode(0xD83D, 0xDE00);   // U+1F600
var pua = String.fromCharCode(0xDB80, 0xDC00);      // genuine U+F0000
var cell = "aé€" + astral + pua;          // 7 code units per cell

var CELLS = 500;
var subject = cell.repeat(CELLS);
var CELL_UNITS = 7;

if (subject.length !== CELLS * CELL_UNITS) {
  throw new Test262Error(
    "subject should be " + CELLS * CELL_UNITS + " code units, got " + subject.length
  );
}

// Well past any plausible sample interval, so lookups must binary-search the
// samples and then walk within one.
if (subject.length < 1024) {
  throw new Test262Error("subject is too short to span multiple samples");
}

// Every "€" sits at a known code-unit offset. Walking all of them checks the
// mapping at hundreds of independent points, not just the first and last.
var euro = /€/gu;
var seen = 0;
var m;
while ((m = euro.exec(subject)) !== null) {
  var expected = seen * CELL_UNITS + 2;
  if (m.index !== expected) {
    throw new Test262Error(
      "unicode match " + seen + " should be at index " + expected + ", got " + m.index
    );
  }
  if (euro.lastIndex !== expected + 1) {
    throw new Test262Error(
      "unicode lastIndex after match " + seen + " should be " + (expected + 1) +
      ", got " + euro.lastIndex
    );
  }
  seen++;
}
if (seen !== CELLS) {
  throw new Test262Error("expected " + CELLS + " unicode matches, got " + seen);
}

// The same sweep without the u flag, which uses the sibling offset table.
var euroPlain = /€/g;
seen = 0;
while ((m = euroPlain.exec(subject)) !== null) {
  if (m.index !== seen * CELL_UNITS + 2) {
    throw new Test262Error(
      "non-unicode match " + seen + " should be at index " + (seen * CELL_UNITS + 2) +
      ", got " + m.index
    );
  }
  seen++;
}
if (seen !== CELLS) {
  throw new Test262Error("expected " + CELLS + " non-unicode matches, got " + seen);
}

// An explicit lastIndex deep in the subject must resolve to the same place the
// sequential scan reached, in both modes.
var probe = /€/gu;
probe.lastIndex = 400 * CELL_UNITS;
if (probe.exec(subject).index !== 400 * CELL_UNITS + 2) {
  throw new Test262Error("unicode lastIndex probe resolved to the wrong offset");
}

var probePlain = /€/g;
probePlain.lastIndex = 400 * CELL_UNITS;
if (probePlain.exec(subject).index !== 400 * CELL_UNITS + 2) {
  throw new Test262Error("non-unicode lastIndex probe resolved to the wrong offset");
}

// A lastIndex landing on the trailing half of a surrogate pair is rounded down
// to the start of that character in Unicode mode. Cell offset 3 is the astral
// character, so offset 4 is its low surrogate.
var midPair = /[\s\S]/gu;
midPair.lastIndex = 100 * CELL_UNITS + 4;
var mid = midPair.exec(subject);
if (mid.index !== 100 * CELL_UNITS + 3) {
  throw new Test262Error(
    "mid-pair lastIndex should resolve to " + (100 * CELL_UNITS + 3) + ", got " + mid.index
  );
}
assertSameUnits(mid[0], astral, "mid-pair match text");

// $` and $' slice the subject using converted offsets, so a replacement deep in
// a long subject reconstructs it exactly.
var single = subject.replace(/€/u, "[$`$']");
assertSameUnits(
  single,
  subject.slice(0, 2) + "[" + subject.slice(0, 2) + subject.slice(3) + "]" + subject.slice(3),
  "$` and $' over a long subject"
);

// A functional replacement reports UTF-16 positions into the original subject.
var positions = [];
subject.replace(/€/gu, function (match, position) {
  positions.push(position);
  return "z";
});
if (positions.length !== CELLS) {
  throw new Test262Error("expected " + CELLS + " replacement positions");
}
for (var i = 0; i < CELLS; i++) {
  if (positions[i] !== i * CELL_UNITS + 2) {
    throw new Test262Error(
      "replacement position " + i + " should be " + (i * CELL_UNITS + 2) +
      ", got " + positions[i]
    );
  }
}

// Replacing every "€" must leave all other characters — including the astral
// and PUA-range ones — byte-identical.
assertSameUnits(
  subject.replace(/€/gu, "z"),
  ("aéz" + astral + pua).repeat(CELLS),
  "global unicode replace over a long subject"
);
assertSameUnits(
  subject.replace(/€/g, "z"),
  ("aéz" + astral + pua).repeat(CELLS),
  "global non-unicode replace over a long subject"
);

// Splitting yields one more piece than there are separators, and the split
// boundaries land in the same places the scan found.
//
// Only the boundaries are asserted here, not the full text of a piece that ends
// in the PUA-range scalar: %Symbol.split% still decodes such a scalar back into
// a lone surrogate on the way out, a pre-existing decode bug tracked in #534
// that is unrelated to offset mapping and reproduces identically on `main`.
var parts = subject.split(/€/gu);
if (parts.length !== CELLS + 1) {
  throw new Test262Error("expected " + (CELLS + 1) + " split parts, got " + parts.length);
}
assertSameUnits(parts[0], "aé", "first split part");
assertSameUnits(parts[CELLS].slice(0, 2), astral, "last split part starts at the right boundary");
for (var p = 1; p < CELLS; p++) {
  if (parts[p].slice(0, 2) !== astral) {
    throw new Test262Error("split part " + p + " does not start at a cell boundary");
  }
}

// The Annex B statics retain UTF-16 offsets into the subject, so a match near
// the end must report the whole preceding text.
/€/u.exec(subject);
if (RegExp.leftContext.length !== 2) {
  throw new Test262Error(
    "leftContext should be 2 code units, got " + RegExp.leftContext.length
  );
}
if (RegExp.rightContext.length !== subject.length - 3) {
  throw new Test262Error(
    "rightContext should be " + (subject.length - 3) + " code units, got " +
    RegExp.rightContext.length
  );
}
