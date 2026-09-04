# Locale-backed `Intl.DateTimeFormat` output

## Problem

`Intl.DateTimeFormat` resolves non-English locales but formats many values with
English symbol tables and hand-built patterns. This makes the resolved locale
disagree with `format()` and `formatToParts()`, and loses locale-specific date
and time glue such as English `at`.

ECMA-262 delegates locale-sensitive date methods to ECMA-402. ECMA-402's
`PartitionDateTimePattern` requires output parts to follow the effective locale
and selected formatting options, while leaving the locale data
implementation-defined. JSSE already uses ICU4X for other `Intl` services.

## Design

Add an ICU4X formatting seam in `datetimeformat.rs`:

- Translate supported `DtfOptions` combinations into an ICU4X dynamic field
  set and formatter preferences.
- Construct an ISO local date/time from the already-resolved JS time zone and
  let ICU4X convert it to the requested calendar.
- Include time-zone data when requested so ICU4X also selects localized
  date/time/zone glue and names.
- Collect ICU4X's annotated output into ECMA-402-style `{ type, value }` parts.
  `format()` joins the same parts, keeping it consistent with
  `formatToParts()`.
- Fall back to the existing JSSE formatter when ICU4X cannot represent an
  arbitrary ECMA-402 component combination or cannot format an edge-case
  value. The fallback preserves current conformance instead of rejecting a
  previously accepted format.

The initial seam covers date/time styles and the common explicit field sets
supported by ICU4X: standalone year/month/weekday, month-day, year-month,
year-month-day, their weekday forms, time, and supported time-zone styles.
Narrow fields and component sets outside ICU4X's semantic field-set model stay
on the existing path.

## Alternatives

Loading only localized month, weekday, era, and day-period symbols would be a
smaller change, but it would retain the incorrect hand-built field ordering,
punctuation, spacing, and date/time glue.

Replacing the existing formatter entirely would remove duplicated behavior,
but ICU4X's semantic field sets cannot express every independent ECMA-402
component combination. A wholesale replacement would therefore risk
regressions in already-passing test262 cases.

## Validation

Repository-owned tests will assert:

- French long month and weekday names.
- Russian standalone month names.
- Locale-specific date/time glue in both `format()` and `formatToParts()`.
- Concatenating `formatToParts()` values equals `format()`.

The existing DateTimeFormat test262 directory and full project quality gate
remain regression checks. The full test262 run determines whether README pass
statistics need updating.
