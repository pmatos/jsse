// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-temporal-totemporaltimezoneidentifier
description: Offset time zone identifiers reject trailing junk
features: [Temporal]
---*/

// TimeZoneNumericUTCOffset as an identifier is exactly ±HH, ±HHMM, or ±HH:MM.
// Trailing characters make the string invalid on every entry point.
const junk = ["+01:00Z", "+01.", "+010"];

for (const tz of junk) {
  assert.throws(RangeError, () => new Temporal.ZonedDateTime(0n, tz), `ZonedDateTime ${tz}`);
  assert.throws(
    RangeError,
    () => new Temporal.Instant(0n).toString({ timeZone: tz }),
    `Instant.toString ${tz}`
  );
}

// Valid forms still accepted and normalized.
assert.sameValue(new Temporal.ZonedDateTime(0n, "+01").timeZoneId, "+01:00", "±HH");
assert.sameValue(new Temporal.ZonedDateTime(0n, "+0130").timeZoneId, "+01:30", "±HHMM");
assert.sameValue(new Temporal.ZonedDateTime(0n, "+01:45").timeZoneId, "+01:45", "±HH:MM");
