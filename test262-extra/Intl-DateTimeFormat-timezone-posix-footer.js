// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-intl.datetimeformat.prototype.formattoparts
description: >
  Intl.DateTimeFormat's timeZoneName offset and named styles evaluate a tzif
  file's POSIX footer rule past the last tabulated transition, instead of
  freezing at the final tabulated offset/abbreviation.
locale: [en-US]
---*/

// September (Northern-hemisphere DST season), well past chrono-tz's ~2099
// transition table.
const farFutureSummer = new Date(Date.UTC(2160, 8, 13, 12, 0, 0));
const farFutureWinter = new Date(Date.UTC(2160, 0, 13, 12, 0, 0));

function tzNamePart(date, timeZone, timeZoneName) {
  const parts = new Intl.DateTimeFormat("en-US", {
    hour: "numeric",
    timeZone,
    timeZoneName,
  }).formatToParts(date);
  return parts.find((part) => part.type === "timeZoneName").value;
}

assert.sameValue(
  tzNamePart(farFutureSummer, "CET", "shortOffset"),
  "GMT+2",
  "CET's recurring daylight-saving offset renders past the transition table"
);
assert.sameValue(
  tzNamePart(farFutureWinter, "CET", "shortOffset"),
  "GMT+1",
  "CET's recurring standard-time offset renders past the transition table"
);

assert.sameValue(
  tzNamePart(farFutureSummer, "America/New_York", "short"),
  "EDT",
  "America/New_York's recurring daylight-saving abbreviation renders past the transition table"
);
assert.sameValue(
  tzNamePart(farFutureWinter, "America/New_York", "short"),
  "EST",
  "America/New_York's recurring standard-time abbreviation renders past the transition table"
);
assert.sameValue(
  tzNamePart(farFutureSummer, "America/New_York", "shortOffset"),
  "GMT-4",
  "America/New_York's recurring daylight-saving offset renders past the transition table"
);
