/*---
esid: sec-regexp.prototype-%symbol.split%
description: >
  RegExp result text preserves genuine supplementary Private Use Area code
  points in the subject.
info: |
  RegExp.prototype [ %Symbol.split% ] ( string, limit )

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
