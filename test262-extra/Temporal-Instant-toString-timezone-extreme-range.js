// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-temporal.instant.prototype.tostring
description: Named zones never silently render as UTC at the extreme end of the Instant range
features: [Temporal]
---*/

// Near the maximum Instant epoch, jiff's representable range (~9999 years) is
// exceeded. The offset must still come from the zone's own recurring POSIX
// footer rule (via 400-year Gregorian cycle shifting into a fixed anchor
// window past every zone's tabulated data), never the bare-0 fallback that
// renders named zones as UTC.
const maxInstant = new Temporal.Instant(8640000000000000000000n);

// +275760-09-13 (UTC) falls in September, which is DST season for both CET
// (Northern-hemisphere summer) and America/New_York — pinning the exact
// recurring-rule offset, not just the absence of the silent-UTC fallback.
assert.sameValue(
  maxInstant.toString({ timeZone: "CET" }),
  "+275760-09-13T02:00:00+02:00",
  "CET resolves its recurring daylight-saving offset at the range edge"
);
assert.sameValue(
  maxInstant.toString({ timeZone: "America/New_York" }),
  "+275760-09-12T20:00:00-04:00",
  "America/New_York resolves its recurring daylight-saving offset at the range edge"
);
assert.sameValue(
  maxInstant.toString({ timeZone: "UTC" }),
  "+275760-09-13T00:00:00+00:00",
  "UTC is unaffected"
);
