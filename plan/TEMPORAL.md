# Temporal: Path to 100% (4482/4482)

**Current state:** 4423/4482 passing (98.68%), 59 failing

## Current Pass Rates by Subdirectory

| Subdirectory | Passing | Total | Rate |
|---|---|---|---|
| Duration | 489 | 522 | 93.68% |
| Instant | 459 | 459 | 100% |
| Now | 66 | 66 | 100% |
| PlainDate | 635 | 635 | 100% |
| PlainDateTime | 749 | 750 | 99.87% |
| PlainMonthDay | 194 | 194 | 100% |
| PlainTime | 481 | 481 | 100% |
| PlainYearMonth | 496 | 496 | 100% |
| ZonedDateTime | 849 | 874 | 97.14% |
| Top-level + toStringTag | 5 | 5 | 100% |

---

## Phase 1: IANA Time Zone Support (+3 tests → 4423/4482) ✅ COMPLETE

**Root cause:** jsse only supports UTC and fixed UTC offsets. Tests that construct
`new Temporal.ZonedDateTime(0n, "CET")` fail with `RangeError: Invalid time zone`.

**Fix applied:** Replaced manual `is_iana_timezone` heuristic (which required `/` in name)
with validation against `chrono-tz`'s full IANA database via `resolve_iana_timezone()`.
Fast path uses `parse::<Tz>()` (hash lookup), slow path does case-insensitive scan.

**Key implementation points:**
- `ToTemporalTimeZoneIdentifier` must accept IANA names (not just UTC/offsets)
- `GetOffsetNanosecondsFor(timeZone, instant)` must return the correct offset for named zones
- `GetPossibleInstantsFor(timeZone, dateTime)` must handle DST transitions (0, 1, or 2 instants)
- `GetStartOfDay` relies on `GetPossibleInstantsFor` for correct behavior near DST boundaries

**Tests expected to pass:**
```
ZonedDateTime/prototype/equals/no-fractional-minutes-hours.js
ZonedDateTime/prototype/since/no-fractional-minutes-hours.js
ZonedDateTime/prototype/until/no-fractional-minutes-hours.js
```

---

## Phase 2: TZ String Validation in relativeTo Property Bags (+14 tests → 4437/4482)

**Root cause:** When a property bag with a `timeZone` property is provided as `relativeTo`,
jsse fails to validate the time zone value per `ToTemporalTimeZoneIdentifier`. Invalid
values should throw but are silently accepted.

**What needs validation:**
1. **Type check:** `timeZone` must be a string → reject null, boolean, number, bigint with TypeError
2. **Bare datetime strings:** A string like `"2021-08-19T17:30"` (no Z, no offset, no bracket)
   is NOT a valid time zone identifier → throw RangeError
3. **Sub-minute offsets:** `"+00:00:01"` is not a valid TZ identifier → throw RangeError
4. **Leap seconds in TZ strings:** e.g. `"2021-08-19T17:30:45.123456789+00:00[UTC]"` with
   `:60` seconds → throw RangeError
5. **Invalid offset strings:** The `offset` property in the bag must be a valid offset string

**Tests expected to pass:**
```
Duration/compare/relativeto-propertybag-timezone-string-datetime.js
Duration/compare/relativeto-propertybag-timezone-string-leap-second.js
Duration/compare/relativeto-propertybag-timezone-wrong-type.js
Duration/compare/relativeto-propertybag-invalid-offset-string.js
Duration/compare/relativeto-sub-minute-offset.js
Duration/prototype/round/relativeto-propertybag-timezone-string-datetime.js
Duration/prototype/round/relativeto-propertybag-timezone-string-leap-second.js
Duration/prototype/round/relativeto-propertybag-timezone-wrong-type.js
Duration/prototype/round/relativeto-propertybag-invalid-offset-string.js
Duration/prototype/round/relativeto-sub-minute-offset.js
Duration/prototype/total/relativeto-propertybag-timezone-string-datetime.js
Duration/prototype/total/relativeto-propertybag-timezone-string-leap-second.js
Duration/prototype/total/relativeto-propertybag-timezone-wrong-type.js
Duration/prototype/total/relativeto-propertybag-invalid-offset-string.js
Duration/prototype/total/relativeto-sub-minute-offset.js
```

---

## Phase 3: Infinity Rejection in relativeTo Property Bags (+3 tests → 4440/4482)

**Root cause:** When a `relativeTo` property bag contains `Infinity` or `-Infinity` for
temporal fields (year, month, day, hour, etc.), jsse should throw RangeError but doesn't.

**Fix:** `ToIntegerWithTruncation` (or wherever temporal fields are extracted from property
bags for relativeTo) must reject `±Infinity` with RangeError.

**Tests expected to pass:**
```
Duration/compare/relativeto-propertybag-infinity-throws-rangeerror.js
Duration/prototype/round/relativeto-infinity-throws-rangeerror.js
Duration/prototype/total/relativeto-infinity-throws-rangeerror.js
```

---

## Phase 4: ZonedDateTime Epoch Nanosecond Range Checks (+10 tests → 4450/4482)

**Root cause:** ZonedDateTime string parsing doesn't validate that the resulting epoch
nanoseconds falls within the valid range `[-8.64×10¹⁸, +8.64×10¹⁸]` ns. Strings
representing dates at the extreme edges of the representable range should throw RangeError.

**Fix:** After parsing a ZonedDateTime ISO string and computing epoch nanoseconds, validate
the result is within the valid range. This check goes in `ToTemporalZonedDateTime` and in
`ToTemporalRelativeToOption` (the ZonedDateTime branch).

Also applies to `relativeTo` string parsing in Duration methods — a relativeTo string that
resolves to an out-of-range ZonedDateTime must throw RangeError.

**Tests expected to pass:**
```
Duration/compare/relativeto-string-limits.js
Duration/prototype/round/relativeto-string-limits.js
Duration/prototype/total/relativeto-string-limits.js
Duration/prototype/total/relativeto-date-limits.js
ZonedDateTime/compare/argument-string-limits.js
ZonedDateTime/from/argument-string-limits.js
ZonedDateTime/prototype/equals/argument-string-limits.js
ZonedDateTime/prototype/since/argument-string-limits.js
ZonedDateTime/prototype/until/argument-string-limits.js
```

**Note:** Only 9 listed above but `relativeto-date-limits.js` is also in this category = 10 total.

---

## Phase 5: Boundary Arithmetic Range Checks (+8 tests → 4458/4482)

**Root cause:** Operations near the representable limits should throw RangeError when the
result would be out of range, but jsse silently produces out-of-range results.

**Fixes needed:**
1. **`AddZonedDateTime`**: Validate result epoch ns is within valid range
2. **`GetStartOfDay`**: Throw RangeError if the computed start-of-day is out of valid limits
3. **`NudgeToCalendarUnit` / `CalendarDateAdd`**: Throw RangeError if `end` date is out of range
4. **`AddDateTime` with large `roundingIncrement`**: Throw RangeError if result is out of range
5. **`roundingIncrement` addition**: When computing the upper bound for rounding, check that
   the addition doesn't overflow the valid epoch ns range

**Tests expected to pass:**
```
Duration/compare/throws-when-target-zoned-date-time-outside-valid-limits.js
Duration/prototype/round/next-day-out-of-range.js
Duration/prototype/total/throws-if-date-time-invalid-with-plaindate-relative.js
Duration/prototype/total/throws-if-date-time-invalid-with-zoneddatetime-relative.js
Duration/prototype/total/throws-if-target-nanoseconds-outside-valid-limits.js
ZonedDateTime/prototype/hoursInDay/get-start-of-day-throws.js
ZonedDateTime/prototype/hoursInDay/next-day-out-of-range.js
ZonedDateTime/prototype/round/day-rounding-out-of-range.js
ZonedDateTime/prototype/since/roundingincrement-addition-out-of-range.js
ZonedDateTime/prototype/until/roundingincrement-addition-out-of-range.js
```

---

## Phase 6: Property Bag Property-Read Order (+13 tests → 4471/4482)

**Root cause:** The spec requires properties on a property bag to be accessed in alphabetical
order via `PrepareCalendarFields` / `GetTemporalRelativeToOption`. jsse reads them in
a non-alphabetical order (e.g., `timeZone` before `calendar`, skipping time fields for
PlainDate context).

**Key spec requirements:**
- All temporal fields must be read in alphabetical order: `calendar`, `day`, `hour`,
  `microsecond`, `millisecond`, `minute`, `month`, `monthCode`, `nanosecond`, `offset`,
  `second`, `timeZone`, `year`
- For PlainDate relativeTo, ALL fields (including time fields) must still be read even
  though they aren't used — the spec reads all fields before checking `timeZone` presence
- The `observable-get-overflow-argument-primitive.js` test also checks that options are read
  even when the call should fail early

**Tests expected to pass:**
```
Duration/compare/order-of-operations.js
Duration/prototype/round/order-of-operations.js
Duration/prototype/total/order-of-operations.js
ZonedDateTime/compare/order-of-operations.js
ZonedDateTime/from/order-of-operations.js
ZonedDateTime/from/observable-get-overflow-argument-primitive.js
ZonedDateTime/prototype/equals/order-of-operations.js
ZonedDateTime/prototype/since/order-of-operations.js
ZonedDateTime/prototype/until/order-of-operations.js
ZonedDateTime/prototype/add/order-of-operations.js
ZonedDateTime/prototype/subtract/order-of-operations.js
ZonedDateTime/prototype/round/order-of-operations.js
ZonedDateTime/prototype/toString/order-of-operations.js
```

---

## Phase 7: Options Property-Read Order and Extra Reads (+7 tests → 4478/4482)

**Root cause:** Options are read in the wrong order or options that shouldn't be read are read.

**Issues:**
1. **ZDT add/subtract**: Reads `disambiguation`, `offset`, `overflow` but spec only reads `overflow`
2. **ZDT round**: Reads `roundingIncrement` AFTER `roundingMode`/`smallestUnit` instead of before
3. **ZDT toString**: Reads options in wrong order; missing `fractionalSecondDigits` read entirely.
   Spec order: `calendarName`, `fractionalSecondDigits`, `offset`, `roundingMode`,
   `smallestUnit`, `timeZoneName`

**Tests expected to pass:**
```
ZonedDateTime/prototype/add/options-read-before-algorithmic-validation.js
ZonedDateTime/prototype/subtract/options-read-before-algorithmic-validation.js
ZonedDateTime/prototype/round/options-read-before-algorithmic-validation.js
ZonedDateTime/prototype/toString/options-read-before-algorithmic-validation.js
```

**Note:** The remaining order-of-operations tests for add/subtract/round/toString overlap
with Phase 6 — some are counted there. The 4 listed here are the ones specifically about
options ordering that aren't already counted in Phase 6.

---

## Phase 8: Duration Round/Balance Correctness Bugs (+4 tests → 4482/4482)

**Root cause:** Individual algorithm bugs producing incorrect results.

### 8a: largestUnit collapsing
`Duration(5,5,5,5,5,5,5,5,5,5).round({largestUnit:"days", smallestUnit:"days", relativeTo})`
returns `years=5` instead of `years=0`. When `largestUnit` is smaller than the duration's
calendar units, those units must be collapsed downward.

**Test:** `Duration/prototype/round/largestunit-smallestunit-combinations-relativeto.js`

### 8b: Week rounding with month balancing
Rounding to 1-week increment with months produces `weeks=2` instead of expected `weeks=3`.

**Test:** `Duration/prototype/round/round-and-balance-calendar-units-with-increment-disallowed.js`

### 8c: Negative month-day balancing
`b.until(a, {largestUnit:"months"})` gives `days=-27` instead of expected `days=-30`.
The negative direction DifferenceISODate month-day balancing is wrong.

**Test:** `PlainDateTime/prototype/until/balance.js`

### 8d: Floating-point precision in Duration.total
`Duration(1,0,0,0,1).total({unit:'years', relativeTo:PlainDate(2020,2,29)})` returns
`1.0001141552511417` instead of `1.0001141552511414`. Precision issue in fractional total.

**Test:** `Duration/prototype/total/rounding-window.js`

---

## Summary: Expected Progress by Phase

| Phase | Fix | Tests Fixed | Cumulative | Rate |
|---|---|---|---|---|
| Baseline | — | — | 4420/4482 | 98.62% |
| Phase 1 | IANA time zones | +3 | 4423/4482 | 98.68% |
| Phase 2 | TZ string validation | +14 | 4437/4482 | 99.00% |
| Phase 3 | Infinity rejection | +3 | 4440/4482 | 99.06% |
| Phase 4 | Epoch ns range checks | +10 | 4450/4482 | 99.29% |
| Phase 5 | Boundary arithmetic | +8 | 4458/4482 | 99.46% |
| Phase 6 | Property-read order | +13 | 4471/4482 | 99.75% |
| Phase 7 | Options-read order | +7 | 4478/4482 | 99.91% |
| Phase 8 | Duration correctness | +4 | 4482/4482 | 100.00% |

**Note:** Some phases may have overlapping test fixes (order-of-operations tests may require
both Phase 6 and Phase 7 fixes). The counts above are best estimates; some tests may shift
between phases when partial fixes are applied.
