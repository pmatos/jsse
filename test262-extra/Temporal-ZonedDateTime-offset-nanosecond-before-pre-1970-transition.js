// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-getnamedtimezoneoffsetnanoseconds
description: >
  The offset one nanosecond before a named-zone offset transition is the
  pre-transition offset, including for pre-1970 (negative epoch) transitions
  where the containing second is the floor, not the truncation toward zero,
  of the instant.
features: [Temporal]
---*/

// America/Los_Angeles observed DST from 1965-04-25T09:00:00Z (a pre-1970,
// i.e. negative-epoch, instant).
const transition = Temporal.Instant.from("1965-04-25T09:00:00Z");
const nsPerHour = 60n * 60n * 1000n ** 3n;
const zone = "America/Los_Angeles";

assert.sameValue(
  transition.toZonedDateTimeISO(zone).offsetNanoseconds,
  Number(-7n * nsPerHour),
  "the transition instant itself has the post-transition offset"
);
assert.sameValue(
  new Temporal.Instant(transition.epochNanoseconds - 1n).toZonedDateTimeISO(zone)
    .offsetNanoseconds,
  Number(-8n * nsPerHour),
  "one nanosecond before the transition has the pre-transition offset"
);
assert.sameValue(
  new Temporal.Instant(transition.epochNanoseconds + 1n).toZonedDateTimeISO(zone)
    .offsetNanoseconds,
  Number(-7n * nsPerHour),
  "one nanosecond after the transition has the post-transition offset"
);

// Europe/Paris moved from Paris Mean Time (+00:09:21) to WET at
// 1911-03-10T23:50:39Z.
const parisTransition = Temporal.Instant.from("1911-03-10T23:50:39Z");
assert.sameValue(
  new Temporal.Instant(parisTransition.epochNanoseconds - 1n)
    .toZonedDateTimeISO("Europe/Paris").offsetNanoseconds,
  Number((9n * 60n + 21n) * 1000n ** 3n),
  "one nanosecond before Paris' 1911 transition has the Paris Mean Time offset"
);
assert.sameValue(
  parisTransition.toZonedDateTimeISO("Europe/Paris").offsetNanoseconds,
  0,
  "the 1911 transition instant itself has the post-transition (WET) offset"
);
