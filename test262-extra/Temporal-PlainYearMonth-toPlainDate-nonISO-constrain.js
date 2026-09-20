// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-temporal.plainyearmonth.prototype.toplaindate
description: >
  toPlainDate resolves the day with overflow "constrain" for non-ISO calendars
  and rejects a day below 1 on every calendar
features: [Temporal]
---*/

function ym(year, month, calendar) {
  return Temporal.PlainYearMonth.from({ year, month, calendar });
}

// CalendarDateFromFields is called with overflow "constrain": a day past the
// end of the month is clamped to the calendar's last day of that month.
assert.sameValue(
  ym(2024, 2, "japanese").toPlainDate({ day: 30 }).toString(),
  "2024-02-29[u-ca=japanese]",
  "day 30 in a leap February is constrained to 29"
);
assert.sameValue(
  ym(2023, 2, "japanese").toPlainDate({ day: 31 }).toString(),
  "2023-02-28[u-ca=japanese]",
  "day 31 in a non-leap February is constrained to 28"
);
assert.sameValue(
  ym(2024, 4, "japanese").toPlainDate({ day: 31 }).toString(),
  "2024-04-30[u-ca=japanese]",
  "day 31 in a 30-day month is constrained to 30"
);
assert.sameValue(
  ym(2024, 2, "japanese").toPlainDate({ day: 1000 }).toString(),
  "2024-02-29[u-ca=japanese]",
  "a very large day is constrained to the last day of the month"
);
assert.sameValue(
  ym(2024, 2, "gregory").toPlainDate({ day: 30 }).toString(),
  "2024-02-29[u-ca=gregory]",
  "day 30 in a leap February is constrained on the gregory calendar"
);

// In-range days are unaffected.
assert.sameValue(
  ym(2024, 2, "japanese").toPlainDate({ day: 15 }).toString(),
  "2024-02-15[u-ca=japanese]",
  "an in-range day is kept"
);
assert.sameValue(
  ym(2024, 2, "japanese").toPlainDate({ day: 29 }).toString(),
  "2024-02-29[u-ca=japanese]",
  "the last valid day is kept"
);

const constrained = ym(2024, 2, "japanese").toPlainDate({ day: 30 });
assert.sameValue(constrained.calendarId, "japanese", "calendar is preserved");
assert.sameValue(constrained.monthCode, "M02", "month is preserved");

// ISO calendar keeps constraining.
assert.sameValue(
  new Temporal.PlainYearMonth(2024, 2).toPlainDate({ day: 30 }).toString(),
  "2024-02-29",
  "ISO: day 30 in a leap February is constrained to 29"
);

// The day is a required positive integer (ToPositiveIntegerWithTruncation via
// PrepareCalendarFields), so constraining must not turn day < 1 into day 1.
const calendars = {
  japanese: ym(2024, 2, "japanese"),
  gregory: ym(2024, 2, "gregory"),
  iso8601: new Temporal.PlainYearMonth(2024, 2),
};
for (const [name, instance] of Object.entries(calendars)) {
  for (const day of [0, -1, -1.5, 0.5, -Infinity, Infinity, NaN]) {
    assert.throws(
      RangeError,
      () => instance.toPlainDate({ day }),
      `${name}: day ${day} throws RangeError`
    );
  }
}
