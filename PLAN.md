# Plan: issue #631 — chrono-tz lacks POSIX footer evaluation

## 1. Problem restated

`chrono-tz` 0.10.4's per-zone transition tables are concrete, materialized
lists of `(epoch_second, offset)` pairs generated at compile time from the
IANA tzdata rules; they stop at a fixed cutoff (empirically ~2099 for
`CET`/`Europe/Paris`; verified by reading the generated
`chrono-tz-0.10.4/src/prebuilt/timezones.rs` table directly — the last `CET`
entry is `(4096573200, FixedTimespan { utc_offset: 3600, dst_offset: 0, name:
"CET" })`, i.e. permanently frozen at *standard* time). Any lookup past that
instant binary-searches off the end of the table and returns that same frozen
entry forever, instead of continuing to apply the zone's recurring DST rule
(the behavior a real TZif file's POSIX footer, or ICU/`node`, provides). Every
call site in jsse that asks "what is the UTC offset of named zone Z at instant
T" — `Temporal.ZonedDateTime`'s offset/epoch-nanosecond conversions,
`Temporal.Instant.prototype.toString({timeZone})`,
`Temporal.ZonedDateTime.prototype.getTimeZoneTransition`,
`Intl.DateTimeFormat`'s offset/name formatting, and the legacy `Date` object's
system-time-zone math — ultimately calls into this frozen tail for any T past
the table, silently diverging from `node`/ICU (e.g. rendering `+01:00`
standard time where the real proleptic rule says `+02:00` DST).

## 2. Spec basis

- `spec/spec.html` `#sec-time-zone-identifiers` ("Time Zone Identifiers"):
  "Implementations that follow the requirements for time zones as described
  in the ECMA-402 ... specification are called time zone aware. Time zone
  aware implementations must support available named time zones corresponding
  to the Zone and Link names of the IANA Time Zone Database, and only such
  names." jsse implements `Intl.DateTimeFormat` and named-zone `Temporal`
  support, so it is a time-zone-aware implementation and this clause applies.
- `spec/spec.html` `#sec-getnamedtimezoneoffsetnanoseconds`
  (`GetNamedTimeZoneOffsetNanoseconds`) and `#sec-getnamedtimezoneepochnanoseconds`
  (`GetNamedTimeZoneEpochNanoseconds`): both are "implementation-defined
  abstract operation"s whose default (UTC-only) bodies are explicitly a
  fallback for implementations *without* political-rule support. The adjacent
  note under `#sec-systemtimezoneidentifier` (spec/spec.html:33745) is the
  operative normative constraint: "`GetNamedTimeZoneEpochNanoseconds` and
  `GetNamedTimeZoneOffsetNanoseconds` must reflect the local political rules
  for standard time and daylight saving time in that time zone, if such rules
  exist." A CET/CEST lookup past chrono-tz's table returns standard time even
  though CET's political rule (recurring DST) still applies — a direct
  violation of this "must" for a time-zone-aware implementation.
- These are the same two ops the `Temporal` proposal (tc39/proposal-temporal;
  not yet merged into the `spec/` submodule at the pinned commit, hence not
  independently cited here, per this repo's convention of citing
  `tc39/proposal-temporal` issue numbers in code comments, e.g.
  `zoned_date_time.rs:3902`) calls directly for named-zone offset/epoch
  resolution — `Temporal.Instant.prototype.toString({timeZone})`,
  `Temporal.ZonedDateTime`'s internal slots, and
  `Intl.DateTimeFormat`'s `timeZone`-aware formatting all bottom out in
  `GetNamedTimeZoneOffsetNanoseconds`/`GetNamedTimeZoneEpochNanoseconds`. No
  JS syntax or semantics beyond what these two ops already define is being
  introduced; this PR only makes jsse's implementation of them correct past
  the table edge.

## 3. Direction chosen, and why the alternative was rejected (with evidence)

The issue names two directions: (a) evaluate the POSIX footer ourselves, or
(b) move zone-offset lookup to a crate that implements footers (`jiff` named
explicitly). A third candidate — extending the "calendar-proxy" trick already
used for the *system* time zone in `helpers.rs::time_zone_datetime_from_time_value`
(search a nearby year in 2072–2099 with the same leap-year-ness and
Jan-1 weekday, so every calendar date lands on the same weekday, then ask
chrono-tz for that proxy year instead) — was investigated and empirically
**rejected**:

- It is *not* the naive "month/day-preserving shift" the issue warns drifts
  the weekday — it deliberately matches weekday-of-year and leap status, and
  a differential probe against `jiff` (ground truth) confirms it reproduces
  the correct offset for `CET`, `America/New_York`, `Australia/Sydney`,
  `America/Santiago`, `Australia/Lord_Howe`, and `Pacific/Chatham` at target
  years 2100/2160/2250 (0 mismatches across a full year of 4-hourly samples
  per zone/year).
- It **fails hard** for zones whose DST rule is not a repeating
  Gregorian-calendar rule. `Africa/Casablanca` observes a Ramadan-linked
  standard-time carve-out: tzdata enumerates explicit per-year transitions
  that drift ~11 days earlier every Gregorian year, with no periodic
  Gregorian recurrence at all. Past its last enumerated transition, a real
  POSIX footer *cannot* encode Ramadan, so `zic` (and therefore `jiff`, and
  therefore `node`/ICU) collapses it to a fixed offset. The probe shows the
  proxy technique disagrees with `jiff` on **essentially every sample**
  (1460/1464/1460 of ~1460 samples/year at 2100/2160/2250) because it
  picks up whatever DST status happened to hold in the arbitrary proxy year,
  applying it as if permanent. Adopting this technique as the general fix
  would trade one ICU-parity bug (the issue as filed) for a different,
  broader one (wrong-by-construction for every Ramadan-linked zone, for every
  future date) — the opposite of what this issue asks for.

**Chosen direction: (b), via the `jiff` crate**, specifically its embedded
`jiff-tzdb` backend. Verified directly (throwaway `/tmp` binary, not part of
this repo): `jiff::tz::TimeZone::get("CET")` with
`features = ["std", "tzdb-bundle-always"]` returns `+02:00` for
`2160-09-13T00:00`, matching the issue's expected `node` output, and matches
`jiff` itself for `Africa/Casablanca` returning a permanently fixed offset
(no oscillation) at the same future years — i.e. `jiff` is correct for both
the Gregorian-annual-rule case and the collapsed-footer case, because it
performs genuine POSIX-footer evaluation (`jiff`'s `tz/posix.rs`,
`tz/tzif.rs`, `tz/zic.rs`) rather than approximating it. `jiff`,
`jiff-core`, and `jiff-tzdb` are all `Unlicense OR MIT`, already inside
`deny.toml`'s license allow-list, and pull in no further transitive
dependencies (`cargo tree` shows exactly these three new crates). This is a
date/time *utility* crate exactly like the existing `chrono`/`chrono-tz`
dependency, not a JS parser or engine — permitted under CLAUDE.md.

`chrono`/`chrono-tz` are **kept**, unchanged, for zone-*name* validation and
enumeration (`is_valid_timezone`, `canonicalize_timezone`,
`chrono_tz::TZ_VARIANTS`, `AvailableNamedTimeZoneIdentifiers`-style listing in
`temporal/mod.rs` and `intl/datetimeformat.rs`) — that machinery is not
affected by the footer bug, and touching it is out of scope (see §7).
`jiff` is added purely as the offset/epoch-nanosecond *computation* engine,
replacing chrono-tz only at the specific call sites that compute an offset or
resolve a local wall-clock time for a named zone.

`jiff`'s `Timestamp` type only spans civil years -9999..=9999 (narrower than
chrono's ~±262,000-year range that the existing extreme-range fix, PR #630,
relies on). The existing "shift the epoch by whole 400-year Gregorian cycles"
technique from PR #630 is kept for instants outside that window (Temporal's
Instant range is ~±273,790 years) — 146,097 days is exactly 20,871 weeks, so
the shift is calendar- and weekday-exact, unlike the rejected proxy search.
The shift now composes with `jiff` instead of chrono-tz: e.g. year 275760
shifts to year 2160, which `jiff` now resolves correctly (verified above),
closing the exact gap the issue's own text flags ("year 275760 mod 400 lands
at 2160, still post-table" — true for chrono-tz, no longer a problem once the
lookup at 2160 itself is correct). The trigger for the shift moves from "does
`chrono::Utc.timestamp_opt` fail" (~262k years) to "does
`jiff::Timestamp::new` fail" (~9999 years) — this fires far more often than
before and gets its own slice and test (§4, slice 1).

## 4. Files to touch

Engine:
- `Cargo.toml` — add `jiff = { version = "0.2", default-features = false,
  features = ["std", "tzdb-bundle-always"] }`. `Cargo.lock` updates via
  `cargo build`.
- `src/interpreter/helpers.rs` — new shared primitives (below); rewrite
  `named_time_zone_offset_ms`, `utc_time`, `local_time_zone_abbreviation`;
  retire `time_zone_datetime_from_time_value`'s calendar-proxy branch.
- `src/interpreter/builtins/temporal/zoned_date_time.rs` — rewrite
  `get_tz_offset_ns`, `get_possible_epoch_ns`, `get_total_offset_secs`.
  `find_exact_transition`/`get_next_transition`/`get_previous_transition`/
  `getTimeZoneTransition` are unchanged in structure, fixed transitively.
- `src/interpreter/builtins/intl/datetimeformat.rs` — rewrite `tz_offset_ms`
  and the named-style (`"short"`/`"long"`) branch of `format_tz_name`.

Not touched (in scope of this bug but already correct, or out of scope — see
§7): `src/interpreter/builtins/temporal/mod.rs` (zone-name validation only),
`is_valid_timezone`/`canonicalize_timezone` in `datetimeformat.rs`.

New shared primitives (proposed home: `src/interpreter/helpers.rs`, already
the shared date/time-math utility module and already imported by
`datetimeformat.rs`; `zoned_date_time.rs` gains a new `use` of it):
- `resolve_named_time_zone(tz: &str) -> Option<jiff::tz::TimeZone>` — thin
  wrapper over `jiff::tz::TimeZone::get`.
- `named_time_zone_offset_secs(tz: &jiff::tz::TimeZone, epoch_secs: i64,
  subsec_nanos: u32) -> i32` — the single choke point for the extreme-range
  400-year-cycle shift (retry `jiff::Timestamp::new` on
  `epoch_secs - cycles * GREGORIAN_CYCLE_SECS` when out of `jiff`'s range),
  so the shift is implemented and tested once, not duplicated across three
  call sites.
- `named_time_zone_ambiguous_offsets(tz: &jiff::tz::TimeZone, dt:
  jiff::civil::DateTime) -> AmbiguousOffset`-shaped result — wraps
  `to_ambiguous_timestamp(dt).offset()`, used by both `get_possible_epoch_ns`
  (needs the raw 0/1/2-way list) and `utc_time` (needs `.compatible()`
  disambiguation).

`get_tz_offset_ns`, `named_time_zone_offset_ms`, `tz_offset_ms` become thin
adapters from their own precision (nanosecond `BigInt`, millisecond `f64`,
millisecond `f64` respectively) to `named_time_zone_offset_secs`.

Docs: none required (no new architectural decision — this is a bugfix to an
existing, documented call chain; no new domain vocabulary).

## 5. TDD slices

1. **Temporal named-zone offset lookup (closes the issue's literal repro).**
   - Test: new `test262-extra/Temporal-Instant-toString-timezone-posix-footer-dst.js`
     asserting `Temporal.PlainDateTime.from("2160-09-13T00:00").toZonedDateTime("CET").toInstant().toString({timeZone:"CET"})`
     ends in `+02:00` (not `+01:00`), plus a same-shape assertion for a
     Southern-Hemisphere zone with an opposite DST season (e.g.
     `Australia/Sydney` in its local January) and one for
     `Africa/Casablanca` pinning the *fixed* (non-oscillating) footer-collapsed
     offset at the same future year, so the test suite itself would catch a
     future regression back to the rejected proxy technique.
   - Production: add `resolve_named_time_zone`/`named_time_zone_offset_secs`
     to `helpers.rs`; rewrite `get_tz_offset_ns` in `zoned_date_time.rs` to
     use them (UTC/offset-string fast paths unchanged); widen the
     extreme-range trigger from chrono's range to jiff's `Timestamp::MIN..=MAX`.
   - Also update the stale caveat in the existing
     `test262-extra/Temporal-Instant-toString-timezone-extreme-range.js`
     ("the DST-vs-standard phase ... depends on POSIX footer evaluation
     (tracked separately)") — tighten its `assert.notSameValue(...,
     "+00:00", ...)` to a `assert.sameValue` on the actual correct DST-phase
     offset for `CET`/`America/New_York` at the Instant max, now that this
     issue is what "tracked separately" pointed at. Do not guess that
     expected offset: derive it by asking `jiff` (or the finished jsse
     build once slice 1 lands) for the offset at whatever instant the
     400-year shift actually maps the Instant max onto — recompute the
     shifted residue from the real shift arithmetic rather than assuming it
     lands on the 2160 residue this plan's probe happened to use.

2. **Temporal named-zone epoch-nanoseconds (local→UTC) lookup.**
   - Test: new `test262-extra/Temporal-ZonedDateTime-from-posix-footer-fold.js`
     using a post-2099 fall-back date in `CET` (e.g. `2160-10-25T02:30`)
     with explicit `disambiguation: "earlier"` vs `"later"`, asserting the two
     resolve to instants one hour apart with offsets `+02:00`/`+01:00`
     respectively (today: both collapse to the single frozen `+01:00`
     candidate, since chrono-tz reports no ambiguity past the table).
   - Production: rewrite `get_possible_epoch_ns` in `zoned_date_time.rs`
     using `named_time_zone_ambiguous_offsets`, mapping `Gap → vec![]`,
     `Fold → vec![earlier, later]` (sorted), `Unambiguous → vec![one]` —
     same external contract as today.

3. **`Temporal.ZonedDateTime.prototype.getTimeZoneTransition` past the table.**
   - Test: new `test262-extra/Temporal-ZonedDateTime-getTimeZoneTransition-posix-footer.js`
     calling `getTimeZoneTransition("next")` from an epoch in `CET` shortly
     after 2099, asserting the result is a non-null `ZonedDateTime` at the
     following March/October transition (today: `null`, since the scan never
     detects an offset change past the frozen tail).
   - Production: swap `get_total_offset_secs`'s internal chrono_tz call for
     `named_time_zone_offset_secs` (minimal diff — the surrounding
     day/90-day coarse-scan-then-binary-search structure in
     `find_exact_transition`/`get_next_transition`/`get_previous_transition`
     is untouched, since it already becomes correct once the pointwise
     offset primitive is correct).

4. **Legacy `Date` object system-time-zone math.**
   - Test: `tests/host_time_zone.rs` is the existing oracle and must stay
     green **unmodified** — in particular its
     `new Date(2100, 6, 15, 12).toISOString()` → `"2100-07-15T16:00:00.000Z"`
     and year-275760/-271821 assertions for `America/New_York` currently pass
     via the calendar-proxy trick being retired in this slice, and this
     plan's own probe found 0 mismatches for `America/New_York` at exactly
     this range, so they must keep passing byte-for-byte. If any of these
     golden strings would need to change to pass, that means the new jiff
     path is wrong — fix the code, do not edit the expected string. Add one
     new test function in that file running with `TZ=Africa/Casablanca`,
     asserting `new Date(2160, 0, 1).getTimezoneOffset()` equals the
     absolute value `jiff` reports for `Africa/Casablanca` at that instant
     (this plan's probe found `jiff` reports a fixed `0`-second offset for
     `Africa/Casablanca` across all of 2100/2160/2250 — convert to
     `getTimezoneOffset()`'s inverted-minutes convention, i.e. `0` minutes,
     and pin that absolute value, not merely "unchanged from 2020": 2020
     falls inside Morocco's enumerated Ramadan-linked rule region, where the
     true offset is itself Ramadan-dependent, so comparing two arbitrary
     years could coincidentally match while both being wrong).
   - Production: add `system_time_zone_jiff()` (`OnceLock`-memoized, built
     from the existing `system_time_zone_identifier()` so name resolution is
     unchanged); rewrite `named_time_zone_offset_ms` and `utc_time` to use
     the shared primitives (`utc_time`'s existing hand-rolled
     `MappedLocalTime` Single/Ambiguous/None branching and backward gap-walk
     — collectively implementing "prefer the offset before the transition" —
     is replaced by `.to_ambiguous_zoned(dt).compatible()`, which is `jiff`'s
     documented equivalent semantics); rewrite `local_time_zone_abbreviation`
     using `to_offset_info(...).abbreviation()`; delete
     `time_zone_datetime_from_time_value`'s calendar-proxy fallback branch
     (the direct chrono decode path for in-range recent dates can stay or be
     replaced — implementation stage's call — but the >2099 proxy-year
     search must go). Do not touch `days_in_year`/`week_day`/`time_from_year`
     themselves; they are general ECMA-262 Date algorithms used elsewhere.

5. **`Intl.DateTimeFormat` offset and named-style zone formatting.**
   - Test: new `test262-extra/Intl-DateTimeFormat-timezone-posix-footer.js`
     asserting `new Intl.DateTimeFormat("en-US", {timeZone:"CET",
     timeZoneName:"shortOffset"}).formatToParts(farFutureDate)` yields
     `"GMT+2"` (not `"GMT+1"`) and `timeZoneName:"short"` yields `"CEST"`
     (not `"CET"`) for a date past 2099.
   - Production: rewrite `tz_offset_ms` to delegate to
     `named_time_zone_offset_secs` via `resolve_named_time_zone`; rewrite the
     named-style branch of `format_tz_name` to use
     `to_offset_info(timestamp)` for both the offset and the abbreviation
     instead of chrono_tz's `.format("%Z")`/`.offset()`.

6. **Guard against IANA-name skew between the two tzdata copies.**
   - Test: new `#[test]` in `src/interpreter/helpers.rs`'s test module (or a
     new `tests/jiff_chrono_tz_name_parity.rs`) iterating
     `chrono_tz::TZ_VARIANTS` and asserting
     `jiff::tz::TimeZone::get(name).is_ok()` for every one. This is an
     engine-internal invariant (not JS-observable on its own), so it belongs
     under `cargo test --release`, not test262-extra: without it, a zone name
     that validates via chrono-tz but fails to resolve in `jiff` would
     silently fall back to the exact "+00:00 fallback" this issue's
     predecessor (#630) already fixed once.

## 6. Test surface

- `test262/test/built-ins/Temporal/Instant/prototype/toString/`,
  `test262/test/built-ins/Temporal/ZonedDateTime/` (offset/epoch-nanosecond
  conversion, `getTimeZoneTransition/`), `test262/test/intl402/DateTimeFormat/`
  — run targeted after each slice; these are structural/generic (test262
  cannot assert exact political-rule values for an
  implementation-defined op) so no baseline movement is expected, but they
  guard against regressions in argument handling, branding, and option
  parsing while the internals are rewritten.
- New `test262-extra/` files per slice 1, 2, 3, 5 above (spec-correct
  behavior — genuine footer evaluation — that test262 cannot cover because
  `GetNamedTimeZoneOffsetNanoseconds`/`GetNamedTimeZoneEpochNanoseconds` are
  implementation-defined).
- `tests/host_time_zone.rs` — existing oracle for the legacy `Date`
  system-time-zone path (slice 4); extended, not replaced.
- New `cargo test` for the chrono-tz/jiff name-parity guard (slice 6).
- Full gates before considering the PR done: `cargo build --release`,
  `cargo test --release`, `./scripts/lint.sh`, `uv run python
  scripts/run-test262.py` (full suite — offset computation is used across
  `Date`, `Temporal`, and `Intl`, so a targeted-only run is not sufficient
  here), `cargo deny check advisories bans licenses sources` (new
  dependency).

## 7. Regression risk

- **Shared choke points.** `get_tz_offset_ns` alone has ~20 call sites inside
  `zoned_date_time.rs` (arithmetic, comparison, `toString`, disambiguation,
  `getOffsetNanosecondsFor`); a mistake in the new primitive's floor-division
  from `BigInt` nanoseconds to `(epoch_secs, subsec_nanos)` (negative epochs
  need floor, not truncating, division — the existing code already has this
  exact comment/guard and it must carry over unchanged) would move many
  tests at once, not just the ones in this plan's slices.
- **Ambiguity/disambiguation semantics.** `utc_time`'s hand-rolled
  Single/Ambiguous/None branch encodes a specific "prefer the offset before
  the transition" tie-break; if `jiff`'s `.compatible()` disagrees with it in
  some corner (e.g. a zone with a sub-hour offset change, or two transitions
  within the same UTC day), legacy `Date` parsing/construction for local
  wall-clock times would silently change. `tests/host_time_zone.rs` and
  test262's `Date` fold/gap tests are the guard; run them explicitly, not
  just as part of the full suite, before and after slice 4.
- **Extreme-range composition.** The 400-year-cycle-shift trigger threshold
  moves from chrono's ~262,000-year range down to jiff's ~9,999-year range,
  so it now fires for a much wider set of inputs than before (any Temporal
  Instant or `Date` time value more than ~9999 years from now, not just the
  ~262,000-year extreme). `test262-extra/Temporal-Instant-toString-timezone-extreme-range.js`
  and `tests/host_time_zone.rs`'s year-275760/-271821 assertions are the
  existing guards for the two ends of this range; both must stay green.
- **`getTimeZoneTransition`'s scan bounds.** `get_next_transition`/
  `get_previous_transition` still scan up to `ns_max`/`ns_min` (~±273,790
  years) in 90-day coarse steps; once the pointwise offset is correct, a
  zone with a genuinely-recurring rule will now find transitions arbitrarily
  far in the future where it previously returned `None` quickly — this
  changes performance characteristics (more loop iterations before hitting
  the coarse-step cap) but not correctness; not expected to trip the 120s
  test262 time limit for any single test, but worth watching in CI timing.
- **New dependency surface.** `jiff`'s embedded `jiff-tzdb` is a separate
  compiled snapshot of the IANA Time Zone Database from chrono-tz's (2025b);
  a version-skew mismatch on some obscure zone's *historical* (pre-modern)
  transition is possible in principle but does not affect this issue's
  target (recurring political rules "if such rules exist," per the spec
  note) and is not expected to move `test262-pass.txt` in either direction.
  `cargo deny check` and the name-parity test (slice 6) are the direct
  guards; a full `test262-pass.txt` diff against `origin/main` (read-only —
  not rewritten, per project convention) is the indirect one.
- **Property MOP / GC / bytecode fast path** are not implicated — no new
  `ObjectKind`, no new property shapes, no GC roots, and none of these
  functions are on the bytecode fast path (they are host-timezone math, not
  language-visible objects) — listed here only to record that they were
  considered and ruled out, not because they are touched.

## 8. Out of scope

- Replacing `get_next_transition`/`get_previous_transition`'s hand-rolled
  day/90-day coarse-scan-then-binary-search with `jiff`'s native
  `.following()`/`.preceding()` transition iterators. Once slice 3 lands,
  this is a pure internal simplification (removes on the order of 100 lines)
  with no observable behavior change — a good follow-up, not a bugfix
  bundle.
- Consolidating zone-*name* validation/enumeration
  (`is_valid_timezone`/`canonicalize_timezone`/`chrono_tz::TZ_VARIANTS`) onto
  `jiff-tzdb`'s own name list, which would let `chrono-tz` be dropped
  entirely as a dependency. Not needed to close this issue (name validation
  is not affected by the footer bug) and is a larger, separate risk surface
  (jsse's existing `canonicalize_timezone` static alias table would need to
  be re-verified against `jiff-tzdb`'s canonicalization instead of
  chrono-tz's).
- The pre-existing latent bug this investigation surfaced: `helpers.rs`'s
  now-retired calendar-proxy trick has the identical Ramadan/Hijri-linked
  failure mode as the rejected general fix (§3) for the *system* time zone
  when `TZ` is set to a zone like `Africa/Casablanca` and the queried date is
  more than ~28 years in the future. This plan's slice 4 fixes it as a direct
  side effect of retiring that code path, so no separate follow-up issue is
  needed — noted here only so the fix isn't mistaken for scope creep.
- Any refactor of `NS_PER_SEC`/`NS_PER_DAY` constant duplication across
  `temporal/mod.rs`/`zoned_date_time.rs`/`instant.rs`, or other
  pre-existing style inconsistencies noticed while reading these files.
- Rewording or renaming unrelated to this bug in any file this plan touches.
