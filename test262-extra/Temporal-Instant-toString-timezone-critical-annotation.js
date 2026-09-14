// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-temporal.instant.prototype.tostring
description: Critical flag (!) on time zone annotations is honored
features: [Temporal]
---*/

const instance = new Temporal.Instant(0n);

// Critical flag with a recognized time zone: annotation is used.
const result = instance.toString({ timeZone: "1970-01-01T00:00Z[!America/New_York]" });
assert.sameValue(result.slice(-6), "-05:00", "recognized critical annotation resolves to its offset");

// Critical flag with an unrecognized time zone: must throw, offset is not a fallback.
assert.throws(
  RangeError,
  () => instance.toString({ timeZone: "1970-01-01T00:00+01:00[!Mars/Olympus_Mons]" }),
  "unknown critical annotation is not an available named time zone"
);
