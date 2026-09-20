// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-getnamedtimezoneepochnanoseconds
description: >
  A local wall-clock time that falls in a DST fall-back fold is still
  correctly detected as ambiguous past a tzif file's last tabulated
  transition, evaluating the POSIX footer rule instead of collapsing to a
  single frozen offset.
info: |
  GetNamedTimeZoneEpochNanoseconds is an implementation-defined abstract
  operation, but the adjacent note under SystemTimeZoneIdentifier is
  normative: "GetNamedTimeZoneEpochNanoseconds and
  GetNamedTimeZoneOffsetNanoseconds must reflect the local political rules
  for standard time and daylight saving time in that time zone, if such
  rules exist."
features: [Temporal]
---*/

// 2160-10-26T02:00 CEST -> CET is CET's fall-back transition well past
// chrono-tz's ~2099 table; 02:30 local time occurs twice.
const earlier = Temporal.PlainDateTime.from("2160-10-26T02:30").toZonedDateTime(
  "CET",
  { disambiguation: "earlier" }
);
const later = Temporal.PlainDateTime.from("2160-10-26T02:30").toZonedDateTime(
  "CET",
  { disambiguation: "later" }
);

assert.sameValue(
  earlier.toInstant().toString({ timeZone: "CET" }),
  "2160-10-26T02:30:00+02:00",
  "the earlier occurrence resolves to the pre-transition (DST) offset"
);
assert.sameValue(
  later.toInstant().toString({ timeZone: "CET" }),
  "2160-10-26T02:30:00+01:00",
  "the later occurrence resolves to the post-transition (standard) offset"
);
assert.sameValue(
  later.epochNanoseconds - earlier.epochNanoseconds,
  3600_000_000_000n,
  "the two occurrences of the folded local time are exactly one hour apart"
);
