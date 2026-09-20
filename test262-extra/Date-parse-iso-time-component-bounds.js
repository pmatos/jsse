// Copyright (C) 2026 jsse contributors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
description: >
  Date.parse and the Date constructor return NaN for Date Time String Format
  strings whose hour, minute, or second fields are out of bounds, and accept
  hour 24 only as the end-of-day midnight (24:00, 24:00:00, 24:00:00.000).
esid: sec-date.parse
info: |
  21.4.3.2 Date.parse ( string )
    Strings that are unrecognizable or contain out-of-bounds format element
    values shall cause this function to return NaN.

  21.4.1.32 Date Time String Format
    HH is the number of complete hours that have passed since midnight as two
    decimal digits from 00 to 24.
    mm is the number of complete minutes since the start of the hour as two
    decimal digits from 00 to 59.
    ss is the number of complete seconds since the start of the minute as two
    decimal digits from 00 to 59.
    sss is the number of complete milliseconds since the start of the second
    as three decimal digits.
    "24:00:00" is the end of a day; 00:00 and 24:00 are the two midnights of a
    date, so 1995-02-04T24:00 is the same instant as 1995-02-05T00:00.
    A string containing out-of-bounds or nonconforming elements is not a valid
    instance of this format.
---*/

var nextDay = Date.UTC(2000, 0, 2);

// HH out of range
assert.sameValue(Date.parse("2000-01-01T25:00"), NaN, "T25:00");
assert.sameValue(Date.parse("2000-01-01T25:00Z"), NaN, "T25:00Z");
assert.sameValue(Date.parse("2000-01-01T99:00:00Z"), NaN, "T99:00:00Z");
assert.sameValue(Date.parse("2000-01-01T-5:00Z"), NaN, "negative hour");
assert.sameValue(Date.parse("+002000-01-01T25:00Z"), NaN, "expanded year, T25:00Z");

// mm out of range
assert.sameValue(Date.parse("2000-01-01T00:60"), NaN, "T00:60");
assert.sameValue(Date.parse("2000-01-01T00:60:00Z"), NaN, "T00:60:00Z");
assert.sameValue(Date.parse("2000-01-01T00:99:00Z"), NaN, "T00:99:00Z");

// ss out of range
assert.sameValue(Date.parse("2000-01-01T00:00:60Z"), NaN, "T00:00:60Z");
assert.sameValue(Date.parse("2000-01-01T23:59:60Z"), NaN, "T23:59:60Z");

// HH = 24 is only the end-of-day midnight
assert.sameValue(Date.parse("2000-01-01T24:00:01Z"), NaN, "T24:00:01Z");
assert.sameValue(Date.parse("2000-01-01T24:01Z"), NaN, "T24:01Z");
assert.sameValue(Date.parse("2000-01-01T24:01:00Z"), NaN, "T24:01:00Z");
assert.sameValue(Date.parse("2000-01-01T24:00:00.001Z"), NaN, "T24:00:00.001Z");
assert.sameValue(Date.parse("2000-01-01T24:00:00.1Z"), NaN, "T24:00:00.1Z");

// Valid boundaries stay valid
assert.sameValue(Date.parse("2000-01-01T24:00Z"), nextDay, "T24:00Z");
assert.sameValue(Date.parse("2000-01-01T24:00:00Z"), nextDay, "T24:00:00Z");
assert.sameValue(Date.parse("2000-01-01T24:00:00.000Z"), nextDay, "T24:00:00.000Z");
assert.sameValue(Date.parse("2000-01-01T24:00:00+01:00"), nextDay - 3600000, "T24:00:00+01:00");
assert.sameValue(
  Date.parse("2000-01-01T24:00"),
  new Date(2000, 0, 2).getTime(),
  "T24:00 without offset is local time"
);
assert.sameValue(Date.parse("2000-01-01T00:00:00.000Z"), Date.UTC(2000, 0, 1), "T00:00:00.000Z");
assert.sameValue(
  Date.parse("2000-01-01T23:59:59.999Z"),
  Date.UTC(2000, 0, 2) - 1,
  "T23:59:59.999Z"
);

// The Date constructor shares the parser
assert.sameValue(new Date("2000-01-01T25:00").getTime(), NaN, "new Date T25:00");
assert.sameValue(new Date("2000-01-01T24:00:01Z").getTime(), NaN, "new Date T24:00:01Z");
assert.sameValue(new Date("2000-01-01T24:00:00Z").getTime(), nextDay, "new Date T24:00:00Z");

// Relaxed date-time variant (space separator, unpadded fields)
assert.sameValue(Date.parse("1997-3-8 25:00:00"), NaN, "space form, hour 25");
assert.sameValue(Date.parse("1997-3-8 1:60:00"), NaN, "space form, minute 60");
assert.sameValue(Date.parse("1997-3-8 1:1:60"), NaN, "space form, second 60");
assert.sameValue(Date.parse("1997-3-8 24:00:01"), NaN, "space form, 24:00:01");
assert.sameValue(
  Date.parse("1997-3-8 24:00:00"),
  Date.parse("1997-3-9 0:00:00"),
  "space form, 24:00:00 is the next midnight"
);
assert.sameValue(
  Date.parse("1997-3-8 1:1:1Z"),
  Date.UTC(1997, 2, 8, 1, 1, 1),
  "space form, valid time still parses"
);
