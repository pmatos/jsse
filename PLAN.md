# Plan: issue #562 — Intl.Locale getCalendars lacks region-sensitive preference data

## 1. Problem restated

`Intl.Locale.prototype.getCalendars()` (`src/interpreter/builtins/intl/locale.rs:702-737`)
always returns `["gregory"]` when the locale carries no explicit `ca` Unicode
extension keyword. Per ECMA-402's `CalendarsOfLocale` algorithm, the result
must instead be the CLDR calendar-preference ordering for the locale's
*lookup region* — the region already computed by the region-preference
resolution (`rg` override → region subtag → `sd` subdivision → Add Likely
Subtags → `"001"`) that PR #564 implemented for `getHourCycles`/`getWeekInfo`.
jsse has that region resolution but no region-keyed calendar-preference table,
so every non-`ca`-tagged locale silently collapses to Gregorian regardless of
region (e.g. Thailand should prefer `["buddhist", "gregory"]`). This is a
locale-data gap, not a region-resolution bug — #551/#564 already fixed the
region resolution itself.

Empirically confirmed (Node 26.5.0, ICU/CLDR reference — used only as a data
source per repo authority order, never as algorithm authority): the lookup is
region-only, not language-conditioned. `en-EG`, `ar-EG`, and `und-EG` all
return the identical Egypt ordering; `he-IL`/`en-IL` and `hi-IN`/`en-IN`
likewise agree. The issue body's phrase "selected by language and
RegionPreference" is loosely worded — `CalendarsOfLocale` takes the whole
`loc` conceptually, but the actual data lookup key is the region alone. All
four target test262 files' `info:` excerpts confirm this: none show a
language parameter, and `region-priority.js` holds the language subtag (`fa`)
constant across every priority level it tests.

Also confirmed empirically: an `rg` override to a region with no explicit
CLDR entry does not disable the override and fall back to the base region —
it falls to `["gregory"]` for the override region itself
(`en-TH-u-rg-dezzzz` → `["gregory"]`, not TH's `["buddhist","gregory"]`).
This matches CLDR's data model, where `"001"`/`"gregory"` is inherited
default data for every territory, not "no data". This is the same "broad"
availability reading PR #564 already chose for `getHourCycles`/`getWeekInfo`
via `RegionPreference::lookup_region()`'s `region_has_locale_data` check —
`getCalendars` should reuse `lookup_region()` directly rather than invent a
second, narrower "does this specific table have an entry" semantics.

## 2. Spec basis

This repository vendors `spec/` as `tc39/ecma262` only (see `.gitmodules`);
`Intl.Locale.prototype.getCalendars` and the `CalendarsOfLocale`/
`RegionPreference` abstract operations are defined by ECMA-402 (via the
`Intl.Locale-info` proposal), which is not vendored in this repo. Per the
precedent set by PR #564 (`fix(intl): honor Intl.Locale region overrides`),
the normative text is cited from the ECMA-402 clause names as reproduced
verbatim in the test262 `info:` blocks of the affected tests — the only
in-repo copy of that text, consistent with the CLAUDE.md authority order
(1. ECMAScript spec, 2. test262, 3. node):

- **`CalendarsOfLocale ( loc )`**, steps reproduced in
  `test262/test/intl402/Locale/prototype/getCalendars/region-override.js`
  and `region-priority.js`:
  1. Let `preference` be `RegionPreference(loc.[[Locale]])`.
  2. Let `region` be `preference.[[Region]]`.
  3. Let `regionOverride` be `preference.[[RegionOverride]]`.
  4. If `regionOverride` is not undefined and calendar preference data for
     `regionOverride` are available, let `lookupRegion` be `regionOverride`.
  5. Else let `lookupRegion` be `region`.
  6. (Remainder: look up and return the calendar preference list for
     `lookupRegion`, defaulting to `« "gregory" »` — not itself quoted in the
     test262 excerpts, but required by the four tests' assertions and by the
     already-shipped default-`gregory` behavior for regions without special
     data.)
- **`RegionPreference ( locale )`**, already implemented in
  `compute_region_preference`/`RegionPreference` (`locale.rs:27-70`, landed by
  #564) — unchanged by this issue. Steps reproduced identically across all
  four target test files.

No JavaScript syntax changes. This is a semantics/data-completeness fix to an
existing, already-shipped method — no new spec clause is introduced, only the
missing calendar-preference table that `CalendarsOfLocale` step 6 requires.

## 3. Files to touch

- `src/interpreter/builtins/intl/locale.rs`
  - Add a private `fn calendar_preference_for_region(region: &str) -> Vec<&'static str>`
    data table (CLDR `common/supplemental/supplementalData.xml`
    `<calendarPreferenceData>`, cross-checked empirically via Node 26.5.0's
    ICU across all ISO-3166-1 alpha-2 codes plus UN M49 macro-regions `001`
    and `419` — Node used only as a data-verification oracle, never as
    algorithm authority). Same shape/precedent as the existing
    `get_timezones_for_region` (`locale.rs:1553`) and the `h12_regions`
    inline array in `getHourCycles` (`locale.rs:811`): a `match region { ... }`
    with multi-pattern OR arms per preference group and a `_ => vec!["gregory"]`
    default.
  - Rewire the `getCalendars` native closure (`locale.rs:702-737`) to, when no
    explicit `ca` keyword is present, parse `tag` as an `IcuLocale`, call
    `compute_region_preference(&locale)`, take `.lookup_region()` (the same
    method `getHourCycles`/`getWeekInfo` already use — reusing its existing
    "is data available" semantics rather than adding a second one), and pass
    that into `calendar_preference_for_region`. Mirror `getHourCycles`'s
    `tag.parse::<IcuLocale>().ok().map_or(default, |locale| ...)` fallback
    style for unparsable tags (should not occur for an already-constructed
    `Intl.Locale`, but keeps the two sibling methods consistent).
  - Add a `#[cfg(test)]` unit test module (or extend an existing one in this
    file, if present) asserting `calendar_preference_for_region` against a
    handful of representative regions — added in the same edit as the table
    and the `getCalendars` rewiring, not as a separate prior edit, so the
    function is never left unwired (an unwired private fn would trip the
    project's clippy `-D warnings` pre-edit hook on dead code).
- `test262-extra/Intl-Locale-getCalendars-region-preference.js` (new) —
  literal-value regression test; see §5.
- `README.md` — update the test262 pass count/percentage after the full-suite
  run, per repo convention (Key Rule #5).

No `docs/adr/` or `CONTEXT.md` changes: this follows an existing, already
-documented pattern (region-keyed static preference tables inside
`locale.rs`) rather than introducing new architecture or vocabulary.

## 4. TDD slices

1. **Red:** confirm current failure baseline —
   `uv run python scripts/run-test262.py test262/test/intl402/Locale/prototype/getCalendars/`
   shows the 8 known-failing scenarios (default + strict mode ×
   `likely-subtags-region.js`, `region-override.js`, `region-priority.js`,
   `subdivision-region.js`).
2. **Green (single edit):** in `locale.rs`, add
   `calendar_preference_for_region` (populated with the 14 non-default CLDR
   groups below) together with the `getCalendars` rewiring described in §3,
   plus the `#[cfg(test)]` unit assertions on the table function itself (e.g.
   `calendar_preference_for_region("TH") == vec!["buddhist", "gregory"]`,
   `calendar_preference_for_region("US") == vec!["gregory"]`,
   `calendar_preference_for_region("001") == vec!["gregory"]`). Run
   `cargo test --bin jsse` (per memory: crate is bin-only) to check the unit
   tests, then re-run the targeted test262 directory from step 1 and confirm
   all 8 scenarios flip green.
   - Table content (region groups → ordering), CLDR-sourced and
     Node-cross-checked:
     - `AE|BH|KW|QA` → `gregory, islamic-umalqura, islamic, islamic-civil, islamic-tbla`
     - `AF|IR` → `persian, gregory, islamic, islamic-civil, islamic-tbla`
     - `AL|AZ|MV|TJ|TM|TR|UZ` → `gregory, islamic-civil, islamic-tbla`
     - `BD|DJ|DZ|EH|ER|ID|IQ|JO|KM|LB|LY|MA|MR|MY|NE|OM|PK|PS|SD|SY|TD|TN|YE` → `gregory, islamic, islamic-civil, islamic-tbla`
     - `CN|CX|HK|MO|SG` → `gregory, chinese`
     - `EG` → `gregory, coptic, islamic, islamic-civil, islamic-tbla`
     - `ET` → `gregory, ethiopic`
     - `IL` → `gregory, hebrew, islamic, islamic-civil, islamic-tbla`
     - `IN` → `gregory, indian`
     - `JP` → `gregory, japanese`
     - `KR` → `gregory, dangi`
     - `SA` → `gregory, islamic-umalqura, islamic, islamic-rgsa`
     - `TH` → `buddhist, gregory`
     - `TW` → `gregory, roc, chinese`
     - all other regions (including `001` and unrecognized/override-only
       regions) → `gregory`
3. **Green:** add `test262-extra/Intl-Locale-getCalendars-region-preference.js`
   pinning literal expected arrays (see §5) and run it via
   `uv run python scripts/run-test262.py test262-extra/Intl-Locale-getCalendars-region-preference.js`.
4. **Refactor/verify:** run `./scripts/lint.sh`, `cargo build --release`,
   `cargo test --release`, then the full
   `uv run python scripts/run-test262.py -j 32` and the broader
   `test262/test/intl402/Locale/prototype/` targeted sweep, confirming zero
   regressions against the `origin/main` baseline and the 8 new passes.
   Update `README.md`'s pass count/percentage from the full-suite result.

## 5. Test surface

- Targeted test262: `test262/test/intl402/Locale/prototype/getCalendars/`
  (the 4 files closing this issue) and
  `test262/test/intl402/Locale/prototype/` (regression sweep over sibling
  methods that share `compute_region_preference`).
- Full regression gate: `uv run python scripts/run-test262.py -j 32` (default
  scope: `language/`, `built-ins/`, `annexB/`, `intl402/`).
- Rust-level: `cargo test --bin jsse` for the new
  `calendar_preference_for_region` unit assertions (per memory: bin-only
  crate, `cargo test --bin jsse` not `cargo test`).
- `test262-extra/Intl-Locale-getCalendars-region-preference.js` (new) covers
  spec-correct behavior the four upstream test262 files deliberately don't
  pin as literals (they're comparative/self-referential, by design, to stay
  CLDR-version-agnostic): explicit literal arrays for a stable set of
  regions/locales this table now controls directly —
  - `new Intl.Locale("th-TH").getCalendars()` → `["buddhist", "gregory"]`
  - `new Intl.Locale("en-US").getCalendars()` → `["gregory"]`
  - `new Intl.Locale("und-001").getCalendars()` → `["gregory"]`
  - explicit `ca` keyword still short-circuits region lookup entirely, e.g.
    `new Intl.Locale("th-TH-u-ca-japanese").getCalendars()` → `["japanese"]`
    (already-passing behavior; guards against a regression where the new
    region-table path accidentally shadows the explicit-keyword path)
  - language independence: `new Intl.Locale("ar-EG").getCalendars()` and
    `new Intl.Locale("en-EG").getCalendars()` produce identical arrays.
  Following the existing flat `test262-extra/` naming and structure (e.g.
  `Intl-DateTimeFormat-locale-data.js`, `Intl-DisplayNames-lookups.js`), with
  an `esid`-style header comment naming `CalendarsOfLocale`.

## 6. Regression risk

- **Isolated blast radius:** the only shared machinery touched is
  `RegionPreference::lookup_region()`, called read-only (no changes to
  `compute_region_preference`, `RegionPreference`, or
  `region_has_locale_data`, which `getHourCycles`, `getWeekInfo`, and
  `getTextInfo` also depend on). Those call sites are unmodified and carry no
  regression risk from this change.
- **No new `ObjectKind`, no new GC roots:** `getCalendars` already returns a
  plain array built via the existing `interp.create_array(...)` helper; the
  new table only changes which `&'static str` values are wrapped into
  `JsValue::from_str`, same allocation shape as today.
- **Not on any hot path:** `Intl.Locale.prototype.getCalendars` is a
  native-closure builtin, not part of `eval_expr`/`exec_statement`,
  `property.rs`'s MOP, or the bytecode fast path — no interaction with
  `bytecode_enabled`.
- **`test262-pass.txt`:** only additive — the 8 target scenarios move from
  fail to pass; no other test reads `calendar_preference_for_region` or
  depends on `getCalendars`'s prior always-`gregory` behavior. Baseline
  itself is not rewritten by this plan (read from `origin/main`, no
  `--update-baseline`).
- **Node-compat library harnesses:** none of the currently-green harnesses
  (`luxon`, `moment`, `zod`, etc.) are known to call
  `Intl.Locale.prototype.getCalendars()`; full-suite `cargo test --release`
  and the test262 run are the actual gates, no harness-specific risk
  identified.
- **Data-accuracy risk:** the table is hand-encoded from an empirical CLDR
  cross-check (Node 26.5.0) rather than parsed from a machine-readable CLDR
  source at build time. A future CLDR revision could shift these orderings;
  that risk is accepted, matching the existing precedent of
  `get_timezones_for_region` and `h12_regions`, which carry the same
  hand-encoded-snapshot risk.

## 7. Out of scope

- `getHourCycles/language-priority.js` (tracked separately as #563 — needs
  language+region time data, unrelated table).
- Any refactor to unify `getCalendars`'s new table-driven lookup with
  `getHourCycles`/`getWeekInfo`/`getTextInfo` into a shared
  "region-preference-table" abstraction. They currently have different value
  shapes (single value vs. ordered list) and only one other consumer
  (`getCalendars` itself) would benefit from sharing `lookup_region()`'s
  already-shared call, which this plan already reuses as-is. A shared
  abstraction is premature with only two shapes in play.
- Attempting to source or vendor the complete CLDR `supplementalData.xml`
  machine-readable, or add a build-time CLDR-parsing step/dependency. The
  hand-encoded 14-group table (plus default) is sufficient for spec
  compliance and matches this file's existing data-table precedent.
- Any change to `RegionPreference`, `compute_region_preference`, or
  `region_has_locale_data` (correct and unrelated, per #564/#551).
- Rewriting `test262-pass.txt` (main-branch-only operation).
- Formatting/cleanup unrelated to this change elsewhere in `locale.rs`.
