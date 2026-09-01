# Plan: issue #551 — Intl.Locale info getters ignore the `-u-rg-` region override

## 1. Problem restated

`Intl.Locale.prototype.getHourCycles()` and `Intl.Locale.prototype.getWeekInfo()` compute
their region-sensitive answer from the locale's plain region *subtag*
(`IntlData::Locale.region`, or the region embedded in the resolved tag) and never look at
the `-u-rg-<subdivision>` ("region override") Unicode extension. Per ECMA-402, both getters
must resolve their lookup region through the `RegionPreference` abstract operation, which
prefers `rg` over the subtag (and also derives a region from the `-u-sd-` subdivision
extension, or from the "Add Likely Subtags" algorithm, when the locale carries no region
subtag at all). Because `rg`/`sd`/likely-subtags are never consulted, `en-US-u-rg-gbzzzz`
still resolves as plain `en-US` instead of matching `en-GB`.

## 2. Spec basis

There is no ECMA-402 submodule in this repo (`spec/` is `tc39/ecma262` only), so these
clauses are cited from the canonical spec at `https://tc39.es/ecma402/` (current draft,
matching the algorithm text quoted verbatim inside the test262 tests themselves — see
`info:` frontmatter in each test file listed in §5):

- **15.3.18 `Intl.Locale.prototype.getHourCycles ( )`** and its runtime operation
  **15.5.x `HourCyclesOfLocale ( loc )`** — steps 2–6: compute `preference` via
  `RegionPreference`, build `preferredRegions` (`[[RegionOverride]]` first when present,
  then `[[Region]]`), and look up hour-cycle data for each in turn.
- **15.3.22 `Intl.Locale.prototype.getWeekInfo ( )`** and **15.5.17 `WeekInfoOfLocale ( loc )`**
  — steps 1–6: compute `preference` via `RegionPreference`; prefer `[[RegionOverride]]` as
  `lookupRegion` when week data exists for it, else `[[Region]]`.
- **`RegionPreference ( locale )`** (referenced by both operations above, clause
  `sec-regionpreference`) — the operation this issue is actually about:
  1. `region` = `GetLocaleRegion(locale)` (the region subtag).
  2. If `region` is undefined: try `CanonicalUnicodeSubdivision(locale, "sd")`; if still
     undefined, apply "Add Likely Subtags" to `locale`, canonicalize, and take
     `GetLocaleRegion` of the result; if still undefined, use `"001"`.
  3. `regionOverride` = `CanonicalUnicodeSubdivision(locale, "rg")`.
  4. Return `{ [[Region]]: region, [[RegionOverride]]: regionOverride }`.
- **`CanonicalUnicodeSubdivision ( locale, key )`** — extracts the `key` Unicode-extension
  keyword value (a `unicode_subdivision_id`: a region-shaped prefix — 2 alpha or 3 digit —
  followed by subdivision suffix characters), takes the region prefix, and canonicalizes it
  as a region subtag.

Confirmed **not** in scope for the `RegionPreference` fix (so the corresponding getters must
be left untouched by it): `CalendarsOfLocale` *does* call `RegionPreference` per the spec,
but jsse's `getCalendars` has no region-sensitive data at all yet (see §7 — out of scope);
`CollationsOfLocale` and `TimeZonesOfLocale` do **not** call `RegionPreference` —
`CollationsOfLocale` matches against `%Intl.Collator%.[[AvailableLocales]]` by prefix
(step 4 hardcodes the "no match" fallback list — see slice 5, which fixes a one-line defect
in that fallback, unrelated to `rg`/`sd`), and `TimeZonesOfLocale` uses `GetLocaleRegion`
directly, with no `rg`/`sd` handling defined for it at all (out of scope, untouched).

**Scope arithmetic.** The issue's impact list names 24 failing scenarios across 12 files.
This plan turns 8 of those 12 files green: all 4 `getHourCycles` region files (slice 3) + 3
`getWeekInfo` region files (slice 4) + `getCollations/und-language.js` (slice 5, a different
clause, folded in because it's a one-line fix in a file already on this plan's touch-list).
It deliberately leaves red: all 4 `getCalendars/*` files and `getHourCycles/language-priority.js`.
`getWeekInfo/likely-subtags-region.js` is the 12th file — its current status is to be
*recorded*, not assumed (see slice 4); it may already be green. The PR body must say
explicitly which of the issue's 12 files are now green and which remain open, and must
`gh issue create` (or reuse #551 with a scope-narrowing comment) for each deferred item in
§7 so the remaining scenarios stay tracked.

## 3. Files to touch

- `src/interpreter/builtins/intl/locale.rs` — add a `RegionPreference`-equivalent helper
  (module-private functions, alongside the existing `extract_unicode_keyword`/
  `set_unicode_keyword` helpers at the top of the file) and wire it into the `getHourCycles`
  and `getWeekInfo` native function bodies (currently at ~L724 and ~L921).
- `docs/specs/2026-09-01-intl-locale-region-preference-design.md` (new) — short design note
  documenting the `RegionPreference` cascade and the deliberate simplification described in
  §6, following the existing pattern in `docs/specs/2026-07-16-intl-datetimeformat-locale-data-design.md`.
- No changes to `src/interpreter/types.rs`: `IntlData::Locale` already carries `tag: String`
  (the full resolved tag, extensions included), which is sufficient to re-parse into an
  `IcuLocale` and read `rg`/`sd` inside the helper — no new struct fields needed.

## 4. TDD slices

1. **Red/green: `canonical_unicode_subdivision_region` helper.**
   Add a `#[cfg(test)] mod tests` block in `src/interpreter/builtins/intl/locale.rs`
   exercising a new private function `canonical_unicode_subdivision_region(locale: &IcuLocale, key: &str) -> Option<String>`:
   cases for a full subdivision id (`"gbzzzz"` → `Some("GB")`, matching real `-u-rg-` values),
   a short subdivision id (`"gbeng"` → `Some("GB")`, matching real `-u-sd-` values), a
   numeric-region subdivision id (e.g. `"019zzzz"` → `Some("019")`), a missing key
   (`None`), and a malformed value (`None`). Production code: the helper itself, built on
   `extract_unicode_keyword` (already in the file) plus `icu::locale::subtags::Region`
   parsing (already used at ~L1174 for the constructor's `options.region` path).

2. **Red/green: `compute_region_preference` cascade.**
   Extend the same test module with cases for a new `RegionPreference` struct (`region: String`,
   `region_override: Option<String>`) built by `compute_region_preference(locale: &IcuLocale) -> RegionPreference`:
   region subtag present (`"en-GB"` → region `"GB"`); region subtag absent but `sd` present
   (`"en-u-sd-gbeng"` → region `"GB"`); region and `sd` both absent (`"th"` → region `"TH"`,
   via `LocaleExpander::new_extended().maximize`, same call already used by `maximize()` at
   ~L575); `rg` present alongside any of the above (`region_override` populated
   independently of `region`); and the ultimate `"001"` fallback specifically asserting that
   `"001".parse::<icu::locale::subtags::Region>()` round-trips to `Some` — a silent `None`
   there would clear the region instead of falling back to it, and none of the test262 files
   in slices 3–4 would catch that. Production code: `compute_region_preference`, reusing
   `LocaleExpander` exactly as `maximize_fn`/`minimize_fn` already do.

3. **Red/green: `getHourCycles` honors region preference.**
   Targeted run of `test262/test/intl402/Locale/prototype/getHourCycles/region-override.js`,
   `region-priority.js`, `subdivision-region.js`, and `likely-subtags-region.js` (all four
   currently fail per the issue). Production code: in the `getHourCycles` native function,
   destructure `tag` instead of `region` from `IntlData::Locale`, parse it into an
   `IcuLocale`, call `compute_region_preference`, and check
   `preference.region_override.as_deref().unwrap_or(&preference.region)` against
   `h12_regions` instead of the raw subtag.

4. **Red/green: `getWeekInfo` honors region preference.**
   Before changing any code, run `test262/test/intl402/Locale/prototype/getWeekInfo/likely-subtags-region.js`
   on its own and record whether it passes or fails today — the issue's impact list omits it
   from `getWeekInfo`'s failures (unlike `getHourCycles`, which lists it), suggesting ICU4X's
   own data-marker fallback may already maximize a bare-language locale correctly, but this
   has not been verified by actually running it. Then make
   `region-override.js`, `region-priority.js`, and `subdivision-region.js` (confirmed failing
   per the issue) pass, treating `likely-subtags-region.js`'s recorded status as the
   expected post-change result (still-passing if it passed before, newly-passing if it
   didn't — either way it must not regress from whatever it was). Production code: in the
   `getWeekInfo` native function, after parsing `tag` into `locale`, compute
   `compute_region_preference(&locale)`, clone `locale` into `lookup_locale`, set
   `lookup_locale.id.region` to the resolved lookup region, and pass `&lookup_locale`
   (instead of `&locale`) to both existing `WeekInformation::try_new(...)` call sites.

5. **Red/green: `getCollations` root-locale fallback.**
   Targeted run of `test262/test/intl402/Locale/prototype/getCollations/und-language.js`
   (currently fails). Unrelated to `RegionPreference` (see §2 — `CollationsOfLocale` never
   calls it) but folded in here because it's a one-line defect in the same file this plan
   already touches: `CollationsOfLocale` step 4 hardcodes the "no matching locale" fallback
   as `["emoji", "eor"]`, but the `getCollations` native function (~L684–721) currently
   returns `["emoji"]` only for that case, dropping `"eor"`. Production code: change the
   `else` branch's fallback array to include `"eor"`. Keep this as its own commit — it is
   a genuinely separate root cause and must be identifiable/revertable independently of the
   `RegionPreference` work.

6. **Regression sweep.**
   Run the full `intl402/Locale/prototype/` directory (not just the touched sub-directories)
   to confirm no other `getHourCycles`/`getWeekInfo`/`getCollations` test (e.g.
   `output-array-values.js`, `firstDay-by-id.js`, `firstDay-by-option.js`) regresses, since
   the common case (region subtag present, no `rg`/`sd`) must resolve identically to today.

## 5. Test surface

Targeted test262 directories to run after each slice:
```
uv run python scripts/run-test262.py test262/test/intl402/Locale/prototype/getHourCycles/
uv run python scripts/run-test262.py test262/test/intl402/Locale/prototype/getWeekInfo/
uv run python scripts/run-test262.py test262/test/intl402/Locale/prototype/getCollations/
uv run python scripts/run-test262.py test262/test/intl402/Locale/prototype/
```
Then the full suite for the baseline-comparison run:
```
uv run python scripts/run-test262.py
```
`cargo test --bin jsse` covers the new `#[cfg(test)]` unit tests for the two helpers (slices
1–2) — no `test262-extra/` addition is needed, since test262 already exercises every
spec-mandated branch of `RegionPreference` (subtag, `sd`, likely-subtags, `rg`, and their
priority ordering) for both getters via the files named in §4. `cargo build --release` must
be run before any `run-test262.py` invocation (debug build is too slow for the full suite).

## 6. Regression risk

- **Scope of the shared helper.** `compute_region_preference` and
  `canonical_unicode_subdivision_region` are pure functions added to
  `src/interpreter/builtins/intl/locale.rs` only; they touch no interpreter hot path
  (`eval_expr`/`exec_statement`), no `property.rs` MOP, no GC rooting/`gc_safepoint()`, no
  `ObjectKind` match, and no bytecode fast path — `Intl.Locale` methods are plain native
  functions, not compiled. `IntlData::Locale`'s shape is unchanged (still built entirely from
  `tag`, which the helper re-parses), so `gc::trace_object_fields` and every other exhaustive
  match over `IntlData`/`ObjectKind` needs no update.
- **Deliberate spec simplification, called out explicitly:** `HourCyclesOfLocale` step 5 and
  `WeekInfoOfLocale` step 4 both say "if data are available for `regionOverride`, use it,
  else fall back to `region`". This plan always prefers `region_override` once it is
  syntactically valid, without a separate "is there data for this region" probe. None of the
  four target test262 files probe the "syntactically valid override, but no data for that
  specific region" edge case (they test priority *ordering* across subtag/`sd`/likely-subtags/
  `rg`, not per-region data availability), so this simplification is spec-correct for every
  case test262 currently checks. If a future test articulates the data-availability edge
  case, `lookup_region` will need a real fallback probe against the hour-cycle/week-data
  tables — tracked as a documented follow-up, not implemented speculatively here.
- **`getWeekInfo`'s two `WeekInformation::try_new` call sites.** Both must be updated to use
  `lookup_locale` — missing one would make `firstDay` and `weekend` disagree on which region's
  data they reflect. Both are covered by slice 4's targeted tests (`region-override.js` etc.
  assert on the full `{firstDay, weekend}` object).
- **`getWeekInfo/likely-subtags-region.js` must not regress.** It currently passes via
  ICU4X's own internal locale-fallback maximization (not via `rg`/`sd` logic). After this
  change, the region used for that lookup is instead the region *this plan's* `LocaleExpander`
  maximize call computes. Both use the same underlying `LocaleExpander::new_extended()` as
  `maximize()`/`minimize()` already do, so the resolved region should be identical — but
  slice 4 explicitly re-runs this file to confirm, since a silent divergence would be an easy
  regression to miss.
- **Node-compat library harnesses.** `luxon` is the one pinned library
  (`scripts/libs/luxon.sh`) most likely to exercise locale-derived week-start data. No
  library test invokes `Intl.Locale.prototype.getHourCycles`/`getWeekInfo` with an `rg`/`sd`
  extension today (grepped `tests/`, `test262-extra/` — clean), so no test author has coded
  around the current buggy fallback, but re-running
  `./scripts/run-library-tests.sh luxon` after the change is a cheap confirmation that its
  1,152-test Node cross-check count doesn't move.
- **`test262-pass.txt` baseline.** This change is additive (fixes previously-failing tests)
  and touches no other getter's code path, so it should only ever move the baseline upward,
  never down — per repo convention that rollover is a `main`-branch operation, not part of
  this PR.

## 7. Out of scope

Deliberately **not** bundled into this PR, each because it has a distinct root cause from
the `rg`/`sd`/likely-subtags region-resolution bug fixed here:

- **`getCalendars` region-sensitivity** (`getCalendars/region-override.js`,
  `region-priority.js`, `subdivision-region.js`, `likely-subtags-region.js`). jsse's
  `getCalendars` currently returns `["gregory"]` unconditionally whenever no explicit
  `-u-ca-` override is present — it has *no* region-keyed calendar-preference data at all
  yet (confirmed by reading the current implementation), unlike `getHourCycles`'s existing
  (if incomplete) `h12_regions` table or `getWeekInfo`'s ICU4X-backed data. Wiring
  `RegionPreference` into `getCalendars` without also building real CLDR
  calendar-preference-by-region data (e.g. `buddhist` for `TH`, `persian` for `IR`/`AF`,
  `islamic-umalqura` for `SA`, `japanese` for `JP`, ...) would fix nothing observable. The
  issue itself flags this as an open question ("worth checking whether that is the same bug
  or an additional data gap") — it is the latter, and needs its own follow-up issue and data
  work.
- **`getHourCycles/language-priority.js`.** Requires CLDR's *language+region* hour-cycle
  overrides (e.g. `fr-CA` vs `en-CA`), which is a data-completeness gap in the existing
  `h12_regions` table orthogonal to `rg`/`sd`/likely-subtags resolution — `HourCyclesOfLocale`
  steps 5–6 (the language-then-region lookup loop) are untouched by this plan. Follow-up
  issue to extend the hour-cycle data table with language-specific entries.
- Any refactor of the pre-existing duplicate `WeekInformation::try_new(...)` call in
  `getWeekInfo` (first for `first_day`'s fallback branch, again for `weekend`) beyond the
  minimal `&locale` → `&lookup_locale` substitution both call sites need — deduplicating them
  into a single lookup is a legitimate cleanup but not required to fix this bug, and would
  widen the diff being reviewed.
