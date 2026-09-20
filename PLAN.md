# Plan: issue #674 — Date.parse accepts out-of-range ISO time components

## 1. Problem restated

`parse_iso_date` in `src/interpreter/helpers.rs` (~L2181) parses the `HH`, `mm`, `ss` fields of the Date Time String Format with `str::parse::<i32>()` and feeds them straight into `make_time`, which does no range checking (`make_time` is the spec's MakeTime and must stay unchecked). So `Date.parse("2000-01-01T25:00")`, `"...T24:00:01Z"` and `"...T24:00:00.001Z"` return a time value instead of `NaN`. `parse_date_string` (L2029) is the single entry point for both `Date.parse` (`builtins/date.rs:1261`) and `new Date(string)` (`builtins/date.rs:1142`), so fixing the parser fixes both. The relaxed-format fallback `parse_space_separated_date` (~L2299) has the same unchecked hour/minute/second fields ("1997-3-8 25:00:00") and is fixed in the same PR through a shared helper.

## 2. Spec basis

- §21.4.1.32 Date Time String Format (`sec-date-time-string-format`): `HH` "two decimal digits from 00 to 24", `mm` and `ss` "two decimal digits from 00 to 59", `sss` three digits; "A string containing out-of-bounds or nonconforming elements is not a valid instance of this format." The accompanying note: `00:00` and `24:00` are the two midnights of a date, and `1995-02-04T24:00` is the same instant as `1995-02-05T00:00`.
- §21.4.3.2 Date.parse (`sec-date.parse`): "Strings that are unrecognizable or contain out-of-bounds format element values shall cause this function to return NaN." This also covers the implementation-defined fallback formats, which is why the space-separated fallback is in scope.
- §21.4.1.29 MakeTime / §21.4.1.31 MakeDate: the existing computation after validation is unchanged.

Why hour `24` is valid only as exact midnight (stated from the spec, node is only a confirmation):

- §21.4.1.32 opens by defining the format as "a simplification of the ISO 8601 calendar date extended format"; ISO 8601 permits `24:00:00` only, as the end-of-day midnight.
- §21.4.1.32 closes with "A string containing out-of-bounds or nonconforming elements is not a valid instance of this format": `T24:30` has every element individually in range but is nonconforming, so it is not a valid instance and, per §21.4.3.2, unrecognizable / out-of-bounds input yields `NaN`.
- The §21.4.1.32 note names `00:00` and `24:00` as the two notations for the two midnights of a date (`1995-02-04T24:00` ≡ `1995-02-05T00:00`).

Alternative considered and rejected: accepting `T24:MM[:SS]` with any nonzero remainder because the table's `HH` range (00–24) and `mm`/`ss` ranges (00–59) are each satisfied. It contradicts the end-of-day reading above; node also returns `NaN`. Record this reasoning in the PR body.

## 3. Files to touch

- `src/interpreter/helpers.rs` — add one small private helper (e.g. `iso_time_in_bounds(hour, minute, second, ms) -> bool`: `hour <= 24`, `minute <= 59`, `second <= 59`, and `hour == 24` ⇒ `minute == 0 && second == 0 && ms == 0`; all fields must also be non-negative: today `T-5:00` parses `"-5"` as hour −5 and is accepted, and the new test asserts it becomes `NaN` — that is out-of-bounds input under §21.4.3.2). The non-negative clause deliberately does not touch the `T+5:00` leading-plus leniency, which is a nonconforming-shape issue left to §7. Call it from `parse_iso_date` and `parse_space_separated_date` right after the time fields are parsed and before `make_time`, returning `None`.
  - Returning `None` from `parse_iso_date` falls through to the later heuristics in `parse_date_string`; verify none of them accepts these strings (`parse_tostring_format` needs a weekday/month-name prefix, `parse_space_separated_date` requires a space, `parse_utcstring_format` needs a comma form, `parse_legacy_date` needs `/` or month names). Confirm empirically with the slices below.
  - `ms` must be checked as parsed (from the truncated first 3 fractional digits), so `T24:00:00.0001` is still valid (ms = 0) and `T24:00:00.001` is not. Confirm what the current parser does with digits beyond 3 (it truncates) and keep that.
- `test262-extra/Date-parse-iso-time-component-bounds.js` — new (see §5).
- No `docs/` / `CONTEXT.md` / ADR changes: no new architecture or vocabulary.

## 4. TDD slices

Build first: `cargo build --release -j4` (explicit long timeout). Run a single file with `uv run python scripts/run-test262.py test262-extra/Date-parse-iso-time-component-bounds.js`. Init submodules first if empty: `git submodule update --init --depth 1 test262 spec`.

1. **Red — hour out of range.** Create `test262-extra/Date-parse-iso-time-component-bounds.js` (test262 header: copyright, `description`, `esid: sec-date.parse`, `info` quoting §21.4.1.32 and §21.4.3.2) with `assert.sameValue(Date.parse("2000-01-01T25:00"), NaN)`, `T25:00Z`, `T99:00:00Z`. Fails today. **Green:** add the helper with the `hour <= 24` check and call it in `parse_iso_date`.
2. **Red — minute/second out of range.** Add `T00:60`, `T00:60:00Z`, `T00:00:60Z`, `T23:59:60Z`, `T00:99:00Z` → `NaN`. **Green:** extend helper with `minute <= 59`, `second <= 59`.
3. **Red — hour 24 only as exactly midnight.** Add `T24:00:01Z`, `T24:01Z`, `T24:00:00.001Z`, `T24:01:00Z` → `NaN`. **Green:** add the `hour == 24` ⇒ everything else zero rule.
4. **Guard (should already pass, keep green) — valid boundaries.** `T24:00Z`, `T24:00:00Z`, `T24:00:00.000Z` equal `Date.UTC(2000, 0, 2)`; `T24:00:00.0001Z` (sub-ms digits truncated, ms=0) is a defined decision — assert only if it matches current behaviour and the spec's "three decimal digits" reading is not contradicted; otherwise leave it out of the test and note it in the PR. `T23:59:59.999Z` and `T00:00:00.000Z` are finite and correct. Also `T24:00` with an offset (`+01:00`) and without one (local time: compare with `new Date(2000, 0, 2).getTime()`). Also expanded-year prefixes (`+002000-01-01T25:00Z`) → `NaN`, and negative fields (`2000-01-01T-5:00Z`) → `NaN` (pins the non-negative clause from §3).
5. **Constructor path.** `new Date("2000-01-01T25:00").getTime()` is `NaN` and `new Date("2000-01-01T24:00:00Z").getTime()` is `Date.UTC(2000, 0, 2)` (`946771200000`) — proves the fix reaches `builtins/date.rs:1142`.
6. **Red — relaxed fallback.** Add `Date.parse("1997-3-8 25:00:00")`, `"1997-3-8 1:60:00"`, `"1997-3-8 1:1:60"` → `NaN`, plus a valid `"1997-3-8 24:00:00"` equal to `"1997-3-9 0:00:00"`, and a valid `"1997-3-8 1:1:1"` still parsing (existing behaviour). **Green:** call the same helper from `parse_space_separated_date` (note its `ms_val`/hour fields are `i32` from `parse_one_or_two_digits`; they cannot be negative there, but the helper's non-negative check is harmless).
7. **Refactor (only if needed).** If the two call sites share enough duplicated time-parsing, leave it duplicated — do **not** unify the two parsers in this PR (see §7). Run `./scripts/lint.sh` and `cargo clippy` clean.

## 5. Test surface

- test262 (targeted): `test262/test/built-ins/Date/parse/`, `test262/test/built-ins/Date/` (constructor `value-string*`/`parse` paths), `test262/test/annexB/built-ins/Date/`, and `test262/test/intl402/` only if Date parsing shows up there (spot check). No existing test262 test covers this: `grep -rln "T24:\|T25:\|24:00" test262/test/built-ins/Date test262/test/annexB/built-ins/Date test262/test/intl402/DateTimeFormat` returns nothing (unbounded run). The only `T24:`/`T25:` hits under `test262/test` are Temporal invalid-string tests, which use Temporal's own parser in `builtins/temporal/`, untouched here. Hence the `test262-extra/` file is the only regression guard and no test262 test is expected to flip.
- New: `test262-extra/Date-parse-iso-time-component-bounds.js` (spec clauses §21.4.1.32 and §21.4.3.2, test262 file pattern). There is no dedicated runner; pass the file/directory to `scripts/run-test262.py`. Related existing test: `test262-extra/Date-parse-fallback-preserves-spec-formats.js` — rerun to prove the toString/toUTCString/toISOString round-trips are unaffected.
- Full gate after implementation: `cargo test --release` (separate command), `./scripts/lint.sh`, then the full `uv run python scripts/run-test262.py` to check no regression against the `origin/main` baseline. Run each gate as its own command; don't rebuild the binary while a suite run is in flight.
- Cross-check with node as reference only: `node -e 'console.log(Date.parse("2000-01-01T25:00"), Date.parse("2000-01-01T24:00:00Z"))'`.

## 6. Regression risk

- Baseline (`test262-pass.txt`, read from `origin/main`): this can only turn previously-accepted invalid strings into `NaN`. No Date test262 test exercises these strings (see §5), so expected baseline movement is none; a passing test that regresses in the `built-ins/Date` diff would indicate over-rejection of a valid form. The plan does not touch or update the baseline.
- Shared machinery: only pure string-parsing helpers in `helpers.rs`; `make_time`/`make_day`/`make_date` are untouched (other callers such as `Date.UTC`, setters and `MakeTime` overflow behaviour must stay unchecked). No tree-walker hot path, property MOP, GC rooting, `ObjectKind` match, bytecode, or library harness code involved. `luxon` / `moment` library harnesses call `Date.parse` but only with valid ISO strings; run `./scripts/run-library-tests.sh luxon` only if time permits (optional, slow).
- Behavioural edge: the valid `T24:00[:00[.000]]` forms must keep working (slice 4) — the main correctness risk is over-rejecting.

## 7. Out of scope

- UTC offset validation (`+25:00`, `+00:60`) in both parsers — the spec table says only "`HH:mm`" without ranges; file a follow-up issue if desired.
- `str::parse::<i32>()` accepting a leading `+` in a 2-char field (`T+5:00` parses as hour 5), and other nonconforming-shape leniencies in `parse_iso_date`; separate bug.
- Day-of-month vs. month length (`2000-02-30`) and unifying `parse_iso_date` with `parse_space_separated_date` or with the Temporal ISO parser.
- `parse_tostring_format` / `parse_utcstring_format` `hh:mm:ss` validation: they parse engine-generated `toString`/`toUTCString` output (implementation-defined round-trip formats), not the Date Time String Format; `parse_space_separated_date` is pulled in only because it is the relaxed variant of the ISO date-time shape and shares the field layout. Follow-up issue if desired.
- Any change to `make_time`, `Date.UTC`, setters, or `test262-pass.txt`.
- Formatting-only or unrelated cleanups in `helpers.rs`.
