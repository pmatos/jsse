# Plan: issue #648 — `PlainYearMonth.prototype.toPlainDate` must constrain, not reject

## 1. Problem restated

`Temporal.PlainYearMonth.prototype.toPlainDate({ day })` on a **non-ISO calendar** calls
`calendar_fields_to_iso_overflow(..., "reject")`, so an out-of-range day (e.g. day 30 for
February 2024 in `japanese`) throws `RangeError: Invalid day for this calendar month`. The
operation always resolves with overflow `constrain`; the ISO branch of the same function
already does (`d.min(max_day)`), and `PlainMonthDay.prototype.toPlainDate` already passes
`"constrain"` for its non-ISO branch. Only the non-ISO branch of the YearMonth method is wrong.

The call site is `src/interpreter/builtins/temporal/plain_year_month.rs:1116-1124` at HEAD
(the issue's "~line 508" is stale). No other `"reject"` literal exists in that file; the other
`calendar_fields_to_iso_overflow` callers (`with`, `from`) take the user's `overflow` option
and are correct.

**Hidden coupling that makes the one-word change unsafe on its own.** `d` is computed as
`to_integer_with_truncation(..) as u8` (Rust `f64 as u8` saturates: `0`, negatives → `0`;
`>255` → `255`). Under `"reject"`, `day: 0` on a non-ISO calendar reaches `None` →
`RangeError` (correct, by accident). Under `"constrain"`, `calendar_fields_to_iso_overflow`
ends in `day.min(max_day).max(1)` (`mod.rs:545`), so `day: 0` / `day: -1` would silently become
day 1. The spec requires a `RangeError` (day is a positive integer). The ISO branch has no
`day < 1` check either (`cd = 0.min(max)`, then `iso_date_within_limits` at day 0), so it is a
latent gap of the same shape. The fix therefore = flip the overflow argument **plus** a
`day < 1` guard before the `as u8` narrowing (same guard idiom already used in
`plain_date.rs:1624`, `:1818`).

## 2. Spec basis

`spec/` is tc39/ecma262 and contains **no Temporal clauses** (its only "Temporal" hits are the
date-time string grammar productions, e.g. `TemporalDecimalFraction`). The governing text is the
Temporal proposal spec (tc39/proposal-temporal), which test262 cites via `esid:`:

- **Temporal.PlainYearMonth.prototype.toPlainDate ( item )** — esid
  `sec-temporal.plainyearmonth.prototype.toplaindate` (same esid used by
  `test262/test/built-ins/Temporal/PlainYearMonth/prototype/toPlainDate/default-overflow-behaviour.js`,
  "A nonexistent resulting date is constrained to an existing date"). Steps: RequireInternalSlot
  → TypeError if `item` not an Object → `PrepareCalendarFields(calendar, item, « day », « », « day »)`
  → `ISODateToFields(calendar, isoDate, year-month)` → `CalendarMergeFields` →
  **`CalendarDateFromFields(calendar, mergedFields, constrain)`** → `CreateTemporalDate`.
  The overflow is the literal `constrain`, independent of calendar.
- **PrepareCalendarFields** / **ToPositiveIntegerWithTruncation** — `day` is a required
  positive integer: non-finite → `RangeError`, `< 1` → `RangeError` (justifies the guard).
- **CalendarDateFromFields** (calendar-specific resolution for non-ISO calendars; `constrain`
  clamps day to the month's length) — the behaviour the fix restores.

Node v26.9.0 output (`2024-02-29[u-ca=japanese]`) is corroboration only, not the basis.

## 3. Files to touch

- `src/interpreter/builtins/temporal/plain_year_month.rs` — `toPlainDate` closure
  (~lines 1086-1155): `"reject"` → `"constrain"`; add `day < 1` → `RangeError` guard on the
  truncated `f64` before narrowing to `u8` (covers ISO and non-ISO branches).
- `test262-extra/Temporal-PlainYearMonth-toPlainDate-nonISO-constrain.js` — new (see §4/§5).
- No `docs/`, `CONTEXT.md`, or ADR changes (no architectural decision, no new vocabulary).
- Do **not** touch `spec/`, `test262/`, or `test262-pass.txt`.

## 4. TDD slices

Build first (workspace has no `target/`): `cargo build --release -j2` with an explicit long
timeout (≥ 600000 ms; retry if it hits the limit — incremental state is kept). Fresh
workspaces need `git submodule update --init --depth 1 test262` (already done in planning) to run
test262. Run fmt/clippy/test gates as separate commands, not `&&`-chained.

0. **Empirical check of the one-word flip (before writing any test file).** Apply only
   `"reject"` → `"constrain"`, build, and run the issue's one-liner. Expected
   `2024-02-29[u-ca=japanese]`. Why this matters: in `calendar_fields_to_iso_overflow`
   (`mod.rs:512-524`) the clamp branch is reached only if `calendar_fields_to_iso` returns `None`
   for the out-of-range day. `icu_calendar` 2.1.1 documents that `DateFromFieldsOptions::default()`
   (overflow `None`) **rejects** (`options.rs:32`, doctest: day 31 in September is `Err`), so
   `None` is expected and the clamp runs. If the output is instead `2024-03-01` (ICU rolled over),
   the flip alone is *not* the fix: clamp `day` to the month length at the call site (one
   function) rather than changing the shared helper, and re-derive slice 2's `day: 1000`
   expectation. Revert the flip afterwards so slice 2 starts red.
1. **Characterisation test for `day < 1` (baseline).** Create
   `test262-extra/Temporal-PlainYearMonth-toPlainDate-nonISO-constrain.js`, header per existing
   files (`// Copyright (C) 2026 Paulo Matos...`, `esid: sec-temporal.plainyearmonth.prototype.toplaindate`,
   `features: [Temporal]`). First section: `assert.throws(RangeError, …)` for `day: 0` and
   `day: -1`, on `japanese` (non-ISO) and on `iso8601` (`new Temporal.PlainYearMonth(2024, 2)`),
   plus a check that `day: Infinity` still throws `RangeError`. Run with
   `uv run python scripts/run-test262.py test262-extra/Temporal-PlainYearMonth-toPlainDate-nonISO-constrain.js`.
   Expected today: non-ISO cases pass (via reject); ISO `day: 0` likely yields a `PlainDate` with `iso_day: 0` (`iso_date_within_limits(y, 2, 0)` is probably true) — record the actual output, not just pass/fail
   (if it fails, that is a pre-existing gap the guard in slice 4 will close; note it in the PR).
2. **Red: the bug.** Add the constrain assertions (values derived from calendar arithmetic, not
   copied from Node): `Temporal.PlainYearMonth.from({ year: 2024, month: 2, calendar: "japanese" }).toPlainDate({ day: 30 })`
   → `2024-02-29[u-ca=japanese]` (use `toString()` and/or `TemporalHelpers.assertPlainDate` with
   `includes: [temporalHelpers.js]`); non-leap Feb (`year: 2023`, day 31 → 28); a 30-day month
   (`month: 4`, day 31 → 30); an in-range day (`day: 15`) unchanged; a huge day (`day: 1000`
   → constrained to the month end, exercising `f64 as u8` saturation); result's `calendarId`
   remains `"japanese"`. Optionally add one year-dependent month-length case (e.g. `coptic`
   `monthCode: "M13"`, 6 days in a year ≡ 3 mod 4, else 5); drop it if pinning the year constant
   takes more than a minute — the japanese February cases already cover the clamp. Run: constrain cases fail with
   `RangeError: Invalid day for this calendar month`.
3. **Green: the fix.** Change `"reject"` → `"constrain"` at `plain_year_month.rs:1123`. Re-run
   the test: constrain cases pass, and the slice-1 non-ISO `day: 0` / `day: -1` cases now
   **fail** (silently constrained to day 1). This demonstrates the coupling described in §1.
4. **Green: `day < 1` guard.** After `to_integer_with_truncation` (before `as u8`), if the value
   is `< 1.0` return `RangeError("day must be a positive integer")` — reuse the message/idiom from
   `plain_date.rs`. All slice-1/2 assertions pass on both ISO and non-ISO paths.
5. **Refactor (only if free).** No refactor planned; do not extract a shared helper between
   `PlainYearMonth`/`PlainMonthDay` `toPlainDate` (out of scope). Run `./scripts/lint.sh`.

Commit as `fix(temporal): constrain overflow in PlainYearMonth.prototype.toPlainDate` (PR title,
Conventional Commits; the squash subject is taken verbatim from it). Mention in the PR body:
the stale line number in the issue, the `day < 1` guard and why it is required by the flip.

## 5. Test surface

Targeted test262 runs (all via `uv run python scripts/run-test262.py <dir>`):

- `test262/test/built-ins/Temporal/PlainYearMonth/prototype/toPlainDate/` (incl.
  `default-overflow-behaviour.js`, `order-of-operations.js`, `limits.js`,
  `infinity-throws-rangeerror.js`, `argument-not-object.js`)
- `test262/test/built-ins/Temporal/PlainMonthDay/prototype/toPlainDate/` (sibling, unchanged; sanity)
- `test262/test/built-ins/Temporal/PlainYearMonth/` and `test262/test/intl402/Temporal/PlainYearMonth/`
- `test262/test/intl402/Temporal/PlainMonthDay/`, `test262/test/built-ins/Temporal/PlainDate/`
- then all of `test262/test/built-ins/Temporal/` and `test262/test/intl402/Temporal/`, then the
  full default suite before pushing.

Not covered by test262: there are no `intl402/Temporal/PlainYearMonth/prototype/toPlainDate`
tests, and the built-ins test only checks ISO constrain. Non-ISO constrain and the `day < 1`
RangeError on both branches get the new `test262-extra/` file above (spec clause under test:
`sec-temporal.plainyearmonth.prototype.toplaindate` — CalendarDateFromFields with `constrain`,
and ToPositiveIntegerWithTruncation via PrepareCalendarFields). Run it with
`uv run python scripts/run-test262.py test262-extra/` (no dedicated runner) and
`cargo test --release` for the Rust suite.

## 6. Regression risk

- **Baseline (`test262-pass.txt`)**: expected to move only upward or not at all. Existing tests
  that reach this branch with an out-of-range day previously got a `RangeError`; any that
  expected that would already be wrong per spec. No baseline update is planned or allowed on this
  branch.
- Shared machinery leaned on: `calendar_fields_to_iso_overflow` (its `"constrain"` path with
  `.max(1)` and the leap-month fallbacks) — unchanged code, but now exercised from one more
  caller with `month_code` derived from `iso_to_calendar_fields`; the ICU4X (`icu_calendar`)
  date resolution; `create_plain_date_result`. No interpreter hot paths, property MOP, GC rooting,
  `ObjectKind` matches, or bytecode path is touched. Node-compat library harnesses (luxon,
  moment, …) do not call this method on non-ISO calendars in a way that relied on a throw;
  luxon runs are not part of the gate for this change.
- Order of observable operations (`get day`, `ToNumber` coercion) is unchanged: the guard runs
  after the existing single coercion and adds no extra property reads
  (`order-of-operations.js` must stay green).

## 7. Out of scope

- Deduplicating the two `toPlainDate` implementations or extracting a shared helper (the
  slopo cluster 502 finding); any restructuring of `calendar_fields_to_iso_overflow`.
- `PlainMonthDay.prototype.toPlainDate`, `with`, `from`, `add`/`subtract` paths
  (option-driven overflow, already correct).
- `era`/`eraYear` handling for `PlainYearMonth.toPlainDate` (only `day` is read per spec).
- Moving `test262-pass.txt`, touching `spec/` or `test262/`, formatting-only or unrelated cleanups.
