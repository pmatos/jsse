// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-temporal.instant.prototype.tostring
description: Malformed time zone annotations are rejected
features: [Temporal]
---*/

const instance = new Temporal.Instant(0n);

// Time zone annotation must precede key=value annotations (RFC 9557 order).
assert.throws(
  RangeError,
  () => instance.toString({ timeZone: "1970-01-01T00:00Z[u-ca=iso8601][America/New_York]" }),
  "time zone annotation after a key=value annotation"
);

// Duplicate time zone annotations are invalid.
assert.throws(
  RangeError,
  () => instance.toString({ timeZone: "1970-01-01T00:00Z[America/New_York][Asia/Tokyo]" }),
  "duplicate time zone annotations"
);

// Unrecognized critical key=value annotations are invalid.
assert.throws(
  RangeError,
  () => instance.toString({ timeZone: "1970-01-01T00:00Z[!u-xx=foo]" }),
  "unrecognized critical key=value annotation"
);

// Unterminated annotation.
assert.throws(
  RangeError,
  () => instance.toString({ timeZone: "1970-01-01T00:00Z[UTC" }),
  "unterminated annotation"
);

// Garbage after the closing bracket.
assert.throws(
  RangeError,
  () => instance.toString({ timeZone: "1970-01-01T00:00Z[UTC]junk" }),
  "trailing characters after annotation"
);

// RFC 9557 space separator between date and time is accepted.
const spaceSep = instance.toString({ timeZone: "1970-01-01 00:00:00+01:00" });
assert.sameValue(spaceSep.slice(-6), "+01:00", "space-separated date-time");

// Date-only string with a time zone annotation is accepted.
const dateOnly = instance.toString({ timeZone: "2020-01-01[America/New_York]" });
assert.sameValue(dateOnly.slice(-6), "-05:00", "date-only with time zone annotation");

// A lone calendar annotation is benign and ignored for time zone purposes.
const calOnly = instance.toString({ timeZone: "1970-01-01T00:00Z[u-ca=iso8601]" });
assert.sameValue(calOnly.slice(-6), "+00:00", "calendar-only annotation");
