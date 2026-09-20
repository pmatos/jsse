// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-temporal.zoneddatetime.prototype.gettimezonetransition
description: >
  getTimeZoneTransition finds a zone's recurring DST transitions past a tzif
  file's last tabulated transition, instead of reporting none because the
  frozen tail offset never changes.
features: [Temporal]
---*/

const zdt = Temporal.PlainDateTime.from("2160-01-01T00:00").toZonedDateTime(
  "CET"
);

const next = zdt.getTimeZoneTransition("next");
assert.notSameValue(
  next,
  null,
  "a spring-forward transition exists in 2160, well past the transition table"
);
assert.sameValue(next.toInstant().toString({ timeZone: "CET" }), "2160-03-30T03:00:00+02:00");

const following = next.getTimeZoneTransition("next");
assert.notSameValue(
  following,
  null,
  "a fall-back transition exists later in 2160"
);
assert.sameValue(
  following.toInstant().toString({ timeZone: "CET" }),
  "2160-10-26T02:00:00+01:00"
);

const previous = following.getTimeZoneTransition("previous");
assert.sameValue(
  previous.epochNanoseconds,
  next.epochNanoseconds,
  "walking backward from the fall-back transition finds the spring-forward transition"
);
