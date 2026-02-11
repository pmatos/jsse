# Temporal: Path to 100% (4482/4482)

**Current state:** 4460/4482 passing (99.51%), 22 failing

## Current Pass Rates by Subdirectory

| Subdirectory | Passing | Total | Rate |
|---|---|---|---|
| Duration | 515 | 522 | 98.66% |
| Instant | 459 | 459 | 100% |
| Now | 66 | 66 | 100% |
| PlainDate | 635 | 635 | 100% |
| PlainDateTime | 749 | 750 | 99.87% |
| PlainMonthDay | 194 | 194 | 100% |
| PlainTime | 481 | 481 | 100% |
| PlainYearMonth | 496 | 496 | 100% |
| ZonedDateTime | 860 | 874 | 98.40% |
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

## Phase 2: TZ String Validation in relativeTo Property Bags (+15 tests → 4438/4482) ✅ COMPLETE

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

## Phase 3: Infinity Rejection in relativeTo Property Bags (+18 tests → 4456/4482) ✅ COMPLETE

**Root cause:** When a `relativeTo` property bag contains `Infinity` or `-Infinity` for
time fields (hour, minute, second, millisecond, microsecond, nanosecond), jsse didn't throw
RangeError because those fields were never read in the relativeTo path.

**Fix applied:** In `to_relative_to_date()` (duration.rs), added time field reading and
validation via `to_integer_with_truncation()` for property bag inputs only (skipping Temporal
objects like ZonedDateTime/PlainDate/PlainDateTime that store data internally).

**Tests fixed:** 18 new passes (3 target + 15 bonus from same fix)
```
Duration/compare/relativeto-propertybag-infinity-throws-rangeerror.js
Duration/prototype/round/relativeto-infinity-throws-rangeerror.js
Duration/prototype/total/relativeto-infinity-throws-rangeerror.js
```

---

## Phase 4: ZonedDateTime Epoch Nanosecond Range Checks (+8 tests → 4449/4482) ✅ COMPLETE

**Root cause:** ZonedDateTime string parsing doesn't validate that the resulting epoch
nanoseconds falls within the valid range `[-8.64×10¹⁸, +8.64×10¹⁸]` ns. Strings
representing dates at the extreme edges of the representable range should throw RangeError.

**Fix applied:** Added `CheckISODaysRange` (wall_epoch_days.abs() > 100_000_000) to ZDT
string parsing in both `to_temporal_zoned_date_time_with_options()` and
`from_string_with_options()` for "reject" and "prefer" offset modes. For Duration
relativeTo, extended `to_relative_to_date()` to return optional ZDT epoch_ns and added
range checks in `round`/`total`/`compare`. Added zero-duration early return before range
checks (spec step 12).

**Key implementation points:**
- Two separate ZDT string parsing paths: `to_temporal_zoned_date_time_with_options()` for
  compare/equals/since/until, `from_string_with_options()` for ZonedDateTime.from()
- CheckISODaysRange only for "reject"/"prefer" modes (not "use"/"exact")
- Duration round/total: separate range checks for ZDT (is_valid_epoch_ns) vs PlainDate
  (iso_date_time_within_limits) relativeTo
- Zero-duration early return prevents false range errors on boundary dates

**Tests fixed (8/9 targeted):**
```
Duration/compare/relativeto-string-limits.js ✅
Duration/prototype/round/relativeto-string-limits.js ✅
Duration/prototype/total/relativeto-string-limits.js ✅
Duration/prototype/total/relativeto-date-limits.js ❌ (deferred to Phase 5)
ZonedDateTime/compare/argument-string-limits.js ✅
ZonedDateTime/from/argument-string-limits.js ✅
ZonedDateTime/prototype/equals/argument-string-limits.js ✅
ZonedDateTime/prototype/since/argument-string-limits.js ✅
ZonedDateTime/prototype/until/argument-string-limits.js ✅
```

**Bonus:** +21 additional passes from zero-duration early return and PlainDate range checks
in Duration round/total.

---

## Phase 5: Boundary Arithmetic Range Checks (+11 tests → 4460/4482) ✅ COMPLETE

**Root cause:** Operations near the representable limits should throw RangeError when the
result would be out of range, but jsse silently produces out-of-range results.

**Fixes applied:**
1. **hoursInDay getter**: CheckISODaysRange on today/tomorrow + IsValidEpochNanoseconds on UTC start/next
2. **ZDT.round day case**: CheckISODaysRange on tomorrow before day rounding
3. **NudgeToCalendarUnit (ZDT-only)**: CheckISODaysRange on end boundary in `round_date_duration_with_frac_days`
4. **NudgeToZonedTime (Duration.round)**: CheckISODaysRange on next-day for ZDT + time unit + non-zero duration
5. **Duration.round/total zero-duration early return**: PlainDate always returns P0D; ZDT only for time-only largestUnit
6. **Duration.round/total PlainDate range check**: ISODateTimeWithinLimits at midnight (not noon)
7. **Duration.total range checks**: iso_date_within_limits in total_relative_duration for boundary dates
8. **Duration.compare/total ZDT path**: iso_date_within_limits in duration_total_ns_relative

**Tests now passing:**
```
Duration/compare/throws-when-target-zoned-date-time-outside-valid-limits.js ✅
Duration/prototype/round/next-day-out-of-range.js ✅
Duration/prototype/total/throws-if-date-time-invalid-with-plaindate-relative.js ✅
Duration/prototype/total/throws-if-date-time-invalid-with-zoneddatetime-relative.js ✅
Duration/prototype/total/throws-if-target-nanoseconds-outside-valid-limits.js ✅
ZonedDateTime/prototype/hoursInDay/get-start-of-day-throws.js ✅
ZonedDateTime/prototype/hoursInDay/next-day-out-of-range.js ✅
ZonedDateTime/prototype/round/day-rounding-out-of-range.js ✅
ZonedDateTime/prototype/since/roundingincrement-addition-out-of-range.js ✅
ZonedDateTime/prototype/until/roundingincrement-addition-out-of-range.js ✅
```

---

## Phase 6: Property Bag Property-Read Order (+14 tests → 4474/4482) ✅ DONE

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
Duration/compare/order-of-operations.js ✅
Duration/prototype/round/order-of-operations.js ✅
Duration/prototype/total/order-of-operations.js ✅
ZonedDateTime/compare/order-of-operations.js ✅
ZonedDateTime/from/order-of-operations.js ✅
ZonedDateTime/from/observable-get-overflow-argument-primitive.js ✅
ZonedDateTime/prototype/equals/order-of-operations.js ✅
ZonedDateTime/prototype/since/order-of-operations.js ✅
ZonedDateTime/prototype/until/order-of-operations.js ✅
ZonedDateTime/prototype/add/order-of-operations.js ✅
ZonedDateTime/prototype/subtract/order-of-operations.js ✅
ZonedDateTime/prototype/round/order-of-operations.js (pre-existing failure)
ZonedDateTime/prototype/toString/order-of-operations.js ✅
```

**Actual result:** +14 passes (12/13 target tests + 2 bonus), 0 regressions. 4474/4482 (99.82%).
ZDT.round order-of-operations was already failing before this phase (not a regression).

---

## Phase 7: Options Property-Read Order and Extra Reads ✅

**COMPLETE** — +3 new passes (ZDT 874/874 = 100%, 0 regressions)

**Fixes applied:**
1. ZDT.round: coerce `roundingIncrement` (valueOf) immediately after reading, before
   `roundingMode`/`smallestUnit` reads
2. ZDT.toString: read and coerce `timeZoneName` before validating `smallestUnit`
   (defer "is it a time unit?" check until after all options are read)

**Tests now passing:**
```
ZonedDateTime/prototype/add/options-read-before-algorithmic-validation.js (Phase 6)
ZonedDateTime/prototype/subtract/options-read-before-algorithmic-validation.js (Phase 6)
ZonedDateTime/prototype/round/options-read-before-algorithmic-validation.js ✅
ZonedDateTime/prototype/toString/options-read-before-algorithmic-validation.js ✅
```

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
| Phase 2 | TZ string validation | +15 | 4438/4482 | 99.02% |
| Phase 3 | Infinity rejection | +18 | 4456/4482 | 99.42% |
| Phase 4 | Epoch ns range checks | +29 | 4449/4482 | 99.26% |
| Phase 5 | Boundary arithmetic | +11 | 4460/4482 | 99.51% |
| Phase 6 | Property-read order | +14 | 4474/4482 | 99.82% |
| Phase 7 | Options-read order | +7 | 4477/4482 | 99.89% |
| Phase 8 | Duration correctness | +5 | 4482/4482 | 100.00% |

**Note:** Some phases may have overlapping test fixes (order-of-operations tests may require
both Phase 6 and Phase 7 fixes). The counts above are best estimates; some tests may shift
between phases when partial fixes are applied.
