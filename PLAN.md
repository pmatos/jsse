# Plan: issue #563 — `Intl.Locale.prototype.getHourCycles` lacks language-specific time data

## 1. Problem restated

`Intl.Locale.prototype.getHourCycles()` resolves its answer from a hand-picked,
region-only table (`h12_regions` in `src/interpreter/builtins/intl/locale.rs`,
currently 11 entries: `US CA AU NZ PH IN EG SA CO PK MY`). The governing
algorithm, `HourCyclesOfLocale`, was normatively changed (tc39/ecma402#1086,
merged) to first look up CLDR time-data keyed by *language-region* (e.g.
`fr-CA`), and only fall back to region-only data when no language-specific
entry exists. Because jsse's table has neither the language-region layer nor
most of the region-only entries CLDR actually defines, locales that CLDR
distinguishes by language within the same region — e.g. `fr-CA` (24-hour) vs.
`en-CA` (12-hour) — collapse to the same, region-only answer. This is
independent of the `rg`/`sd` region-*resolution* bug fixed in #551/#564; here
the region is resolved correctly, but the *data keyed on it* is incomplete.

## 2. Spec basis

`Intl.Locale.prototype.getHourCycles` and its `HourCyclesOfLocale` /
`RegionPreference` abstract operations are defined by the TC39 "Intl Locale
Info" proposal (Stage 3), which targets ECMA-402, not ECMA-262. This repo's
`spec/` submodule tracks only `tc39/ecma262` (per `CLAUDE.md`, never
modified), so there is no local `spec/` clause to cite by line number — the
same situation already documented for the sibling region-preference work in
`docs/specs/2026-09-01-intl-locale-region-preference-design.md`. The
authoritative text instead lives in `tc39/ecma402`'s `spec/locale.html`
(proposal repo: `tc39/proposal-intl-locale-info`).

The exact change this issue asks for is normative PR
**tc39/ecma402#1086**, "Normative: Take language subtag into account in
locale hour cycle and calendar lookup" (merged; closes ecma402#1056). Its
description: *"Some of CLDR's hour cycle preference data is indexed by
language and region, not just by region... `new Intl.Locale('fr-CA')
.getHourCycles()` ... after this change, will return an array where element 0
is `h23`, instead of `h12`."* — this is verbatim the issue's `fr-CA`/`en-CA`
example.

The merged `HourCyclesOfLocale ( loc )` algorithm, normalized here from the
PR #1086 diff into the proposal repo's `restricted` /
`CreateArrayFromListOrRestricted` framing used by the surrounding spec text
(`spec/locale.html`, § Intl.Locale.prototype.getHourCycles abstract
operations) — this is a paraphrase for readability, not a verbatim quote of
either source alone:

```
1. Let restricted be loc.[[HourCycle]].
2. Let preference be RegionPreference(loc.[[Locale]]).
3. If preference.[[RegionOverride]] is not undefined, let preferredRegions be
   « preference.[[RegionOverride]], preference.[[Region]] »; else let
   preferredRegions be « preference.[[Region]] ».
4. Let hourCycles be a new empty List.
5. Let language be GetLocaleLanguage(loc.[[Locale]]).
6. For each String region of preferredRegions, do
   a. Let locale be the string-concatenation of language, "-", and region.
   b. If hourCycles is empty and time data for locale locale are available,
      then
      i. Set hourCycles to a List of unique hour cycle identifiers ("h11",
         "h12", "h23", "h24"), sorted in descending preference of those in
         common use for date and time formatting in locale locale.
   c. If hourCycles is empty and time data for region region are available,
      then
      i. Set hourCycles to ... in region region.
7. If hourCycles is empty, set hourCycles to « "h23" ».
8. Return CreateArrayFromListOrRestricted(hourCycles, restricted).
```

(`restricted`/`CreateArrayFromListOrRestricted` — the `loc.[[HourCycle]]`
short-circuit for an explicit `-u-hc-` extension — is already correctly
implemented in jsse; this issue is scoped to step 6's data lookup only.)

`RegionPreference` itself (`region`, `region_override` cascade: region
subtag → `sd` subdivision → likely-subtags → `"001"`, with `rg` as an
independent override) is unchanged by #1086 and is already implemented as
`compute_region_preference` in `locale.rs` (landed in #564). This plan does
**not** touch that function.

"Time data" is UTS #35's
[Time_Data](https://unicode.org/reports/tr35/tr35-dates.html#Time_Data),
concretely CLDR's `common/supplemental/supplementalData.xml`, `<timeData>`
element. Its `regions` attribute already mixes plain region codes (`US`,
`001`, `419`) with language-region pairs using an underscore (`fr_CA`,
`en_001`, `ku_SY`) — exactly the two lookup keys `HourCyclesOfLocale` step 6
needs. Fetched from `unicode-org/cldr` (`main`, current at plan time), full
element for reference (`H`/`h` are the only `preferred` values CLDR
currently assigns anywhere in this table — no `K`/`k` — but the mapping
below stays 4-way for UTS #35 fidelity: `H`→`h23`, `h`→`h12`, `K`→`h11`,
`k`→`h24`):

```xml
<timeData>
  <hours preferred="H" allowed="H" regions="AX BQ CP CZ DK FI ID IS ML NE RU SE SJ SK"/>
  <hours preferred="H" allowed="H h" regions="001 BI BY FO GL HU MG MT MU MV NO PL TH TJ TM VN ZW"/>
  <hours preferred="H" allowed="H h hb hB" regions="AC AI BW BZ CC CK CX DG FK GB GG GI GS IE IM IO JE LT MK MN MS NF NG NR NU PN SH SX TA ZA en_IL"/>
  <hours preferred="H" allowed="H h hB" regions="CF CM LU NP PF SC SM SN TF VA ca_ES fr_CA gl_ES it_CH it_IT"/>
  <hours preferred="H" allowed="H h hB hb" regions="AR CL EA IC KG KM LK MA PY UY af_ZA es_BR es_ES es_GQ"/>
  <hours preferred="H" allowed="H K h" regions="JP"/>
  <hours preferred="H" allowed="H hb hB h" regions="AF LA"/>
  <hours preferred="H" allowed="H hB" regions="AD AM AO AT AW BE BF BJ BL BR CG CI CV CW DE EE FR GA GF GN GP GW HR HT IL IT KZ MC MD MF MQ MZ NC NL PM PT RE RO SI SR ST TG TR WF YT ZM ku_SY"/>
  <hours preferred="H" allowed="H hB h" regions="AZ BA BG CH GE LI ME RS UA UZ XK"/>
  <hours preferred="H" allowed="H hB h hb" regions="ES GQ"/>
  <hours preferred="H" allowed="H hB hb h" regions="CN LV TL zu_ZA"/>
  <hours preferred="H" allowed="hB H" regions="CD IR"/>
  <hours preferred="H" allowed="hB hb H h" regions="KE MM RW TZ UG"/>
  <hours preferred="h" allowed="h H" regions="AS BT DJ ER GH IN LS PG PW SO TO VU WS"/>
  <hours preferred="h" allowed="h H hb hB" regions="CY GR"/>
  <hours preferred="h" allowed="h H hB" regions="AL TD"/>
  <hours preferred="h" allowed="h H hB hb" regions="419 BO CO CR CU DO EC GT HN KP KR MX NI NA PA PE PR SV VE"/>
  <hours preferred="h" allowed="h hb H hB" regions="AG AU BB BM BS CA DM FJ FM GD GM GU GY JM KI KN KY LC LR MH MP MW NZ SB SG SL SS SZ TC TT UM US VC VG VI en_001 en_HK en_MY"/>
  <hours preferred="h" allowed="h hB H" regions="BD PK"/>
  <hours preferred="h" allowed="h hB hb H" regions="AE BH DZ EG EH HK IQ JO KW LB LY MO MR OM PH PS QA SA SD SY TN YE ar_001"/>
  <hours preferred="h" allowed="hb hB h H" regions="BN MY"/>
  <hours preferred="h" allowed="hB h H" regions="hi_IN kn_IN ml_IN te_IN"/>
  <hours preferred="h" allowed="hB h H hb" regions="KH"/>
  <hours preferred="h" allowed="hB h hb H" regions="ta_IN"/>
  <hours preferred="h" allowed="hB hb h H" regions="TW ET gu_IN mr_IN pa_IN"/>
</timeData>
```

This table also *validates* the current `h12_regions` list: every one of its
11 entries (`US CA AU NZ PH IN EG SA CO PK MY`) does appear under a
`preferred="h"` line above — it was a correct but small hand-picked subset,
not wrong data. This plan replaces it with the full table (region-only keys
as-is, `lang_REGION` keys rewritten to hyphenated `lang-REGION` to match the
runtime-built lookup string), which both adds the language-region layer and
fixes region-only coverage gaps (e.g. `GR`, `CY`, `AL`, `TD`, `KH`, `TW`,
`ET`, `BD`, `KP`, `KR`, `MX`, `VE`, `BN` are `h12`-preferred in CLDR but were
previously defaulting to `h23` for lack of a table entry).

## 3. Files to touch

- `src/interpreter/builtins/intl/locale.rs` — the only production file.
  - Replace the `h12_regions` array and its binary `contains()` check inside
    the `getHourCycles` native function body (currently lines ~806–820) with:
    - A module-level static CLDR-derived hour-cycle table (structured as
      `(preferred: &str, keys: &[&str])` groups, transcribed from the XML
      above — plain region codes unchanged, `lang_REGION` tokens rewritten to
      `lang-REGION`).
    - A small pure helper, e.g. `fn hour_cycle_for_key(key: &str) -> Option<&'static str>`,
      placed near `compute_region_preference`/`region_has_locale_data` for
      the same reasons those are free functions (testable in isolation,
      no `Interpreter` dependency).
    - The `preferredRegions` cascade from the algorithm above, built from the
      already-existing `preference.region` / `preference.region_override`
      (no change to `compute_region_preference` or `RegionPreference`
      itself): for each candidate region (override first, if present, then
      the resolved region), try `"{language}-{region}"` before `"{region}"`,
      stopping at the first hit; default to `"h23"` if nothing matches.
    - `language` is `locale.id.language.to_string()`, matching the existing
      pattern at `locale.rs:196`.
  - Extend the existing `#[cfg(test)] mod tests` block (~line 1764) with a
    unit test for `hour_cycle_for_key` (see TDD slice 2).
- `docs/specs/2026-09-01-intl-locale-region-preference-design.md` — update
  the "Scope" paragraph, which currently reads "Language-and-region-specific
  hour-cycle preferences are also absent from the existing region-only table
  and are tracked in #563": once implemented, reword to state this is
  resolved (one sentence), so the doc doesn't dangle a forward reference to a
  closed issue.
- `README.md` — update the default-run test262 pass count/percentage on line
  9 (`Current default run: **99,779 / 99,907 (99.87%)**`, plus its
  surrounding failure-breakdown prose — "The 128 failures are all newly
  added upstream coverage... the remaining 16 are tracked engine gaps" — if
  those specific counts shift too) from the **real output** of the full
  `scripts/run-test262.py` run in TDD slice 3/§5, not by adding 2 to the
  current figures by hand. This is a count bump only — **not** a
  `test262-pass.txt` `--update-baseline` operation (that stays a
  `main`-branch action, out of scope here).

No parser/lexer/AST changes, no new `ObjectKind` variants, no bytecode
changes, no dependency changes.

## 4. TDD slices

1. **Confirm red.** Run
   `uv run python scripts/run-test262.py test262/test/intl402/Locale/prototype/getHourCycles/`.
   Expected (already verified during planning, including manually confirming
   `new Intl.Locale('und-CA'|'fr-CA'|'en-CA').getHourCycles()` all currently
   return `["h12"]` — no throw, no parse gap, purely the missing
   language-region data): 18/20 scenarios pass, with `language-priority.js`
   failing in both default and strict mode; the other 9 files (`name.js`,
   `prop-desc.js`, `branding.js`, `output-array.js`,
   `output-array-values.js`, `region-priority.js`, `region-override.js`,
   `subdivision-region.js`, `likely-subtags-region.js`) already pass and must
   stay passing — they exercise the `RegionPreference` cascade shape, which
   this change also touches, so they double as regression guards.
2. **Helper + table + call site in one edit, red→green via unit test.**
   This repo's pre-commit/PostToolUse hook runs `clippy -D warnings` on every
   `.rs` edit and fails on dead code, so `hour_cycle_for_key` and its static
   table cannot land unwired even temporarily — the `#[cfg(test)]` unit test
   doesn't exempt it from that non-test build. Sequence within a single
   edit pass instead: write the `#[test]` in `mod tests` first (e.g.
   `hour_cycle_for_key_prefers_language_region_over_region_only`, asserting
   `hour_cycle_for_key("GB") == Some("h23")`,
   `hour_cycle_for_key("US") == Some("h12")`,
   `hour_cycle_for_key("CA") == Some("h12")` (region-only),
   `hour_cycle_for_key("fr-CA") == Some("h23")` (language-region override),
   `hour_cycle_for_key("en-CA") == None` (no language-region entry — the
   cascade must fall back to `"CA"` itself, not treat this as `h23`),
   `hour_cycle_for_key("001") == Some("h23")`,
   `hour_cycle_for_key("en-001") == Some("h12")`,
   `hour_cycle_for_key("zz") == None` (unknown region)) — that's the red
   state, since the function doesn't exist yet (`cargo test --bin jsse
   hour_cycle_for_key` fails to compile). Then, before invoking the
   Edit/Write tool again, also write the table + helper + the
   `preferredRegions` cascade in `getHourCycles` (replacing the old
   `h12_regions.contains(&lookup_region)` branch, region-override then
   region, language-region tried before region-only at each step, default
   `"h23"`, reusing `compute_region_preference` unchanged) so the on-disk
   file is never left with an unused function. Green is both
   `cargo test --bin jsse hour_cycle_for_key` and a re-run of the slice-1
   test262 target showing 20/20, with `language-priority.js` now passing in
   both modes and all previously-passing files unaffected.
3. **Docs.** Update the design doc's "Scope" paragraph and the `README.md`
   pass-count line from the real full-suite output (§5), not by arithmetic.
   No behavior change.

## 5. Test surface

- **Primary target:** `test262/test/intl402/Locale/prototype/getHourCycles/`
  (10 files / 20 scenarios) — must reach 20/20 after slice 2.
- **Regression guard:** `test262/test/intl402/Locale/` (broader directory,
  covers `getWeekInfo`, `getCalendars`, `getCollations`,
  `getNumberingSystems`, the constructor's `rg`/`sd` handling, etc. — all
  consumers of `compute_region_preference`/`RegionPreference`, which is
  *not* modified by this change, so this run should show zero deltas besides
  the two `getHourCycles` scenarios).
- **Unit tests:** `cargo test --bin jsse` (crate is bin-only) covering the
  new `hour_cycle_for_key` helper in `locale.rs`'s existing `mod tests`.
- **No new `test262-extra/` file.** Every dimension of the
  `HourCyclesOfLocale`/`RegionPreference` priority cascade (language before
  region, region-override before region, `sd` before likely-subtags, likely-
  subtags before `"001"`) is already exercised by the existing test262 files
  in this directory using their "search for a suitable implementation-
  neutral candidate" pattern — that's precisely what test262 already covers,
  including after this fix. What's *not* independently spec-mandated is the
  literal contents of the CLDR table itself (that's reference data, not
  ECMA-402 semantics); pinning specific known-good entries belongs in the
  Rust unit tests (slice 2), matching how `region_preference_uses_the_
  specified_signal_order` already pins its own facts in the same file.
- **Final gate before opening the PR:** a full `uv run python
  scripts/run-test262.py` run (per `CLAUDE.md`, "After any implementation
  work, run the full test262 suite"). Its real pass count/percentage is what
  slice 3 copies into `README.md` line 9 — do not compute the new figure by
  adding 2 to the old one; the two numbers should agree, but the run is the
  source of truth, not arithmetic on the old count.

## 6. Regression risk

- **Blast radius is one native-function body.** No changes to `eval_expr` /
  `exec_statement`, `property.rs`, `gc.rs`/`gc_safepoint()`, the `ObjectKind`
  match, or the bytecode fast path. `getHourCycles` is a leaf builtin with no
  bytecode-compiled counterpart to keep in sync.
  - Shared machinery: `compute_region_preference` / `RegionPreference` are
    used by `getWeekInfo` (line ~1017) too, but this plan does not modify
    either — only how `getHourCycles` *consumes* the returned
    `RegionPreference`. `getWeekInfo` behavior is unchanged; the broader
    `intl402/Locale/` regression run (§5) is the cheap way to confirm that
    empirically rather than by inspection alone.
  - `datetimeformat.rs`'s `locale_default_hour_cycle` (used for
    `Intl.DateTimeFormat`'s own `hc`/`hour12` default resolution) is a
    separate, independently hand-rolled language-only table — not read by,
    or shared with, `Intl.Locale.prototype.getHourCycles`. Not touched, not
    at risk.
- **Expect a wider *user-visible* change than the two named locales.**
  Replacing the 11-entry table with the full CLDR table changes the
  `h12`/`h23` classification for many regions that had no entry before and
  therefore silently defaulted to `h23` (e.g. `GR`, `CY`, `AL`, `TD`, `KH`,
  `TW`, `ET`, `BD`, `KP`, `KR`, `MX`, `VE`, `BN` become `h12`). This is the
  correct, spec-required outcome (the old table was simply incomplete, not a
  deliberate narrower semantics), but it's worth calling out explicitly in
  the PR description since it's a bigger observable diff than "just `fr-CA`
  vs `en-CA`".
- **`rg` override with no time data changes its fallback path.** The old
  `getHourCycles` code gated the override on `region_has_locale_data`
  (CLDR region-display-name coverage) before using it, e.g. an `rg`
  targeting `019` (a macro-region with no display name, used by the existing
  `canonical_unicode_subdivision_returns_its_region` unit test's
  `en-u-rg-019zzzz` case) fell straight through to the plain region. Under
  the new `preferredRegions` cascade there is no separate availability gate
  before trying the override — the loop tries `"{language}-019"` then
  `"019"` in our hour-cycle table, finds neither (no CLDR time-data entry
  for `019`), and *then* naturally moves on to the plain region, landing on
  the same answer. Net behavior is unchanged for this case, but by a
  different mechanism (empty-lookup fallthrough vs. an explicit
  availability check) — worth a one-line PR note since it's a subtle
  divergence from the old code path, not from the spec.
- **`test262-pass.txt` baseline:** expected net movement is exactly the 2
  `language-priority.js` scenarios (fail→pass); nothing else in the targeted
  or regression-guard directories should move. Per project rule, the runner
  reads the baseline from `origin/main:test262-pass.txt` and this plan does
  not call for `--update-baseline` (that stays a `main`-branch operation).

## 7. Out of scope

- **`getCalendars()`'s equivalent gap.** The same normative PR
  (ecma402#1086) also updates `CalendarsOfLocale` to prefer language-region
  calendar data. The PR's own author notes *"It is not possible to write a
  test for `getCalendars()` since currently there are no known
  implementations where language-region-specific calendar preferences
  exist"* — and the design doc already tracks that independent gap under
  #562. Not bundled here.
- **Modeling CLDR's `allowed` (secondary) hour-cycle list.** This plan keeps
  the existing single-`preferred`-identifier convention (`getHourCycles()`
  returns a one-element array), which every currently-passing test262 file
  already accepts (`output-array-values.js` only requires membership in
  `{h11,h12,h23,h24}`, not the full ordered `allowed` set). Returning the
  full `allowed` list is a legitimate future enhancement, not required to
  close #563, and would be a larger, separately-reviewable change.
  `K`/`k` (`h11`/`h24`) mapping is included in the transcription rule for
  UTS #35 fidelity even though no current CLDR entry uses them as
  `preferred`.
- **`RegionPreference`/`compute_region_preference` refactors.** Untouched;
  any cleanup there belongs to whatever issue motivates it, not this one.
- **Full-suite `test262-pass.txt` baseline rewrite** and any `main`-branch-
  only bookkeeping.
- **General formatting/cleanup** of `locale.rs` beyond the new table and
  cascade.
