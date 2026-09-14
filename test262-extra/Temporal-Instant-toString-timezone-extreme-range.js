// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-temporal.instant.prototype.tostring
description: Named zones never silently render as UTC at the extreme end of the Instant range
features: [Temporal]
---*/

// Near the maximum Instant epoch, chrono's representable range is exceeded.
// The offset must still come from the zone's own data (via 400-year Gregorian
// cycle shifting), never the bare-0 fallback that renders named zones as UTC.
// Note: the DST-vs-standard phase this far past the tz transition table
// depends on POSIX footer evaluation (tracked separately); this test pins
// only the absence of the silent-UTC fallback.
const maxInstant = new Temporal.Instant(8640000000000000000000n);

assert.notSameValue(
  maxInstant.toString({ timeZone: "CET" }).slice(-6),
  "+00:00",
  "CET resolves a zone offset at the range edge, not the UTC fallback"
);
assert.notSameValue(
  maxInstant.toString({ timeZone: "America/New_York" }).slice(-6),
  "+00:00",
  "America/New_York resolves a zone offset at the range edge, not the UTC fallback"
);
assert.sameValue(
  maxInstant.toString({ timeZone: "UTC" }),
  "+275760-09-13T00:00:00+00:00",
  "UTC is unaffected"
);
