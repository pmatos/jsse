// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-intl.datetimeformat.prototype.formatToParts
description: >
  A time zone name that follows date fields without any time field is joined
  to the date the way CLDR joins a date and a time (", ", or "at" after a long
  month name), the same text V8 produces.
---*/

function format(options) {
  return new Intl.DateTimeFormat("en", options).format(0);
}

assert.sameValue(
  format({ timeZone: "Europe/Berlin", timeZoneName: "long" }),
  "1/1/1970, Central European Standard Time"
);
assert.sameValue(
  format({ timeZone: "America/New_York", timeZoneName: "long" }),
  "12/31/1969, Eastern Standard Time"
);
assert.sameValue(
  format({ timeZone: "America/New_York", timeZoneName: "short" }),
  "12/31/1969, EST"
);
assert.sameValue(
  format({ timeZone: "UTC", timeZoneName: "long" }),
  "1/1/1970, Coordinated Universal Time"
);
assert.sameValue(
  format({ timeZone: "Europe/Berlin", year: "numeric", timeZoneName: "long" }),
  "1970, Central European Standard Time"
);
assert.sameValue(
  format({ timeZone: "Europe/Berlin", weekday: "long", timeZoneName: "short" }),
  "Thursday, GMT+1"
);
assert.sameValue(
  format({ timeZone: "Europe/Berlin", month: "short", year: "numeric", timeZoneName: "long" }),
  "Jan 1970, Central European Standard Time"
);
assert.sameValue(
  format({ timeZone: "Europe/Berlin", month: "long", day: "numeric", timeZoneName: "long" }),
  "January 1 at Central European Standard Time"
);
assert.sameValue(
  format({ timeZone: "Europe/Berlin", month: "numeric", timeZoneName: "longOffset" }),
  "1, GMT+01:00"
);

var parts = new Intl.DateTimeFormat("en", {
  timeZone: "Europe/Berlin",
  timeZoneName: "long"
}).formatToParts(0);
assert.sameValue(
  parts.map(function (p) { return p.type; }).join(","),
  "month,literal,day,literal,year,literal,timeZoneName"
);
assert.sameValue(parts[5].value, ", ");

assert(
  /^1:00:00\sAM Central European Standard Time$/.test(
    format({ timeZone: "Europe/Berlin", timeStyle: "full" })
  ),
  "a time field keeps the time and zone adjacent"
);
assert(
  /^1\sAM Central European Standard Time$/.test(
    format({ timeZone: "Europe/Berlin", hour: "numeric", timeZoneName: "long" })
  )
);
