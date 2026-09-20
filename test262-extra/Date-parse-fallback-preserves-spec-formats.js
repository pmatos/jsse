// Copyright (C) 2026 jsse contributors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
description: >
  Date.parse may fall back to implementation-specific heuristics for strings
  outside the Date Time String Format, but that fallback must not perturb the
  formats the specification pins down: the toString, toUTCString and
  toISOString round trips, ISO date-only (UTC) versus date-time without offset
  (local time), and NaN for out-of-bounds or unrecognizable input.
esid: sec-date.parse
info: |
  21.4.3.2 Date.parse ( string )
    The function first attempts to parse the String according to the format
    described in Date Time String Format (21.4.1.32), including expanded years.
    If the String does not conform to that format the function may fall back to
    any implementation-specific heuristics or implementation-specific date
    formats. Strings that are unrecognizable or contain out-of-bounds format
    element values shall cause this function to return NaN.

    If x is any Date whose milliseconds amount is zero within a particular
    implementation of ECMAScript, then all of the following expressions should
    produce the same numeric value in that implementation:
      x.valueOf()
      Date.parse(x.toString())
      Date.parse(x.toUTCString())
      Date.parse(x.toISOString())

  21.4.1.32 Date Time String Format
    When the UTC offset representation is absent, date-only forms are
    interpreted as a UTC time and date-time forms are interpreted as a local
    time.
---*/

var samples = [
  new Date(0),
  new Date(1312408800000),
  new Date(-86400000),
  new Date(Date.UTC(1969, 11, 31, 23, 59, 59)),
  new Date(Date.UTC(2000, 1, 29, 12, 30, 45)),
  new Date(Date.UTC(1999, 4, 1, 0, 0, 0)),
];

samples.forEach(function(x) {
  var expected = x.valueOf();
  assert.sameValue(Date.parse(x.toString()), expected, "toString " + x.toString());
  assert.sameValue(Date.parse(x.toUTCString()), expected, "toUTCString " + x.toUTCString());
  assert.sameValue(Date.parse(x.toISOString()), expected, "toISOString " + x.toISOString());
});

assert.sameValue(Date.parse("2011-08-04"), Date.UTC(2011, 7, 4), "date-only is UTC");
assert.sameValue(
  Date.parse("2011-08-04T00:00:00"),
  new Date(2011, 7, 4).getTime(),
  "date-time without offset is local"
);
assert.sameValue(Date.parse("2011-08-04T00:00:00Z"), Date.UTC(2011, 7, 4), "explicit Z");

[
  "2000-13-01",
  "2000-01-32",
  "1997-3-8T11:19:20",
  "1997-03-08 11",
].forEach(function(s) {
  assert.sameValue(Date.parse(s), NaN, "out-of-bounds " + JSON.stringify(s));
});

["", " ", "invalid date", "foo", "1/1/2000/1", "1//2000"].forEach(
  function(s) {
    assert.sameValue(Date.parse(s), NaN, "unrecognizable " + JSON.stringify(s));
    assert.sameValue(new Date(s).getTime(), NaN, "constructor " + JSON.stringify(s));
  }
);
