// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-temporal.instant.prototype.tostring
description: Slashless IANA time zone names resolve their real offset
features: [Temporal]
---*/

const instance = new Temporal.Instant(0n);

// chrono-tz/IANA fixed and ruled zones without a '/' must resolve, not render as UTC.
assert.sameValue(instance.toString({ timeZone: "CET" }).slice(-6), "+01:00", "CET in January");
assert.sameValue(instance.toString({ timeZone: "EST" }).slice(-6), "-05:00", "EST fixed zone");
assert.sameValue(instance.toString({ timeZone: "cet" }).slice(-6), "+01:00", "case-insensitive canonicalization");
