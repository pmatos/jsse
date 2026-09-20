// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-getnamedtimezoneoffsetnanoseconds
description: >
  Named-zone offset lookups evaluate a tzif file's POSIX footer rule past the
  last tabulated transition, instead of freezing at the final tabulated offset.
info: |
  GetNamedTimeZoneOffsetNanoseconds is an implementation-defined abstract
  operation, but the adjacent note under SystemTimeZoneIdentifier is
  normative: "GetNamedTimeZoneEpochNanoseconds and
  GetNamedTimeZoneOffsetNanoseconds must reflect the local political rules
  for standard time and daylight saving time in that time zone, if such
  rules exist."
features: [Temporal]
---*/

// CET/CEST recurring DST rule, evaluated well past chrono-tz's ~2099 table.
const cetSummer = Temporal.PlainDateTime.from("2160-09-13T00:00")
  .toZonedDateTime("CET")
  .toInstant();
assert.sameValue(
  cetSummer.toString({ timeZone: "CET" }),
  "2160-09-13T00:00:00+02:00",
  "CET is in its recurring daylight-saving phase (+02:00) in September, past the transition table"
);

const cetWinter = Temporal.PlainDateTime.from("2160-01-13T00:00")
  .toZonedDateTime("CET")
  .toInstant();
assert.sameValue(
  cetWinter.toString({ timeZone: "CET" }),
  "2160-01-13T00:00:00+01:00",
  "CET is in its recurring standard-time phase (+01:00) in January, past the transition table"
);

// Southern-hemisphere zone: DST season is the opposite half of the year.
const sydneySummer = Temporal.PlainDateTime.from("2160-01-13T12:00")
  .toZonedDateTime("Australia/Sydney")
  .toInstant();
assert.sameValue(
  sydneySummer.toString({ timeZone: "Australia/Sydney" }),
  "2160-01-13T12:00:00+11:00",
  "Australia/Sydney is in its recurring daylight-saving phase (+11:00) in January, past the transition table"
);

const sydneyWinter = Temporal.PlainDateTime.from("2160-07-13T12:00")
  .toZonedDateTime("Australia/Sydney")
  .toInstant();
assert.sameValue(
  sydneyWinter.toString({ timeZone: "Australia/Sydney" }),
  "2160-07-13T12:00:00+10:00",
  "Australia/Sydney is in its recurring standard-time phase (+10:00) in July, past the transition table"
);

// Africa/Casablanca's Ramadan-linked carve-out is not a repeating
// Gregorian-calendar rule, so a POSIX footer (and therefore a
// spec-conforming implementation) collapses it to a single fixed offset
// past the table, in both the local summer and winter — never oscillating
// the way a naive "nearest matching calendar year" proxy would.
const casablancaSummer = Temporal.PlainDateTime.from("2160-07-13T12:00")
  .toZonedDateTime("Africa/Casablanca")
  .toInstant();
assert.sameValue(
  casablancaSummer.toString({ timeZone: "Africa/Casablanca" }),
  "2160-07-13T12:00:00+00:00",
  "Africa/Casablanca resolves to its fixed footer-collapsed offset in July, past the transition table"
);

const casablancaWinter = Temporal.PlainDateTime.from("2160-01-13T12:00")
  .toZonedDateTime("Africa/Casablanca")
  .toInstant();
assert.sameValue(
  casablancaWinter.toString({ timeZone: "Africa/Casablanca" }),
  "2160-01-13T12:00:00+00:00",
  "Africa/Casablanca resolves to the same fixed footer-collapsed offset in January, past the transition table"
);
