# `Intl.Locale` region preference

## Problem

Region-sensitive `Intl.Locale` info methods used the locale's region subtag
directly. That ignored the Unicode `rg` region override, the `sd` subdivision
when no region subtag exists, and likely-subtag region inference. For example,
`en-US-u-rg-gbzzzz` therefore used US hour-cycle and week data instead of GB
data.

ECMA-402 `RegionPreference` defines one shared cascade:

1. Use the locale's region subtag.
2. If absent, extract the region prefix from the `sd` subdivision keyword.
3. If still absent, add likely subtags and use their region.
4. If no region results, use the world region `001`.

The region prefix of `rg` is resolved independently as the optional override.

## Design

Keep region preference local to `intl/locale.rs` and derive it from the full
stored ICU locale tag. No new `IntlData` slots are needed.

- `canonical_unicode_subdivision_region` validates the UTS 35 subdivision
  shape, extracts its two-letter or three-digit region prefix, and
  canonicalizes that region through ICU4X.
- `compute_region_preference` implements the region-subtag, `sd`, likely
  subtags, and `001` cascade and also returns the independent `rg` override.
- `RegionPreference::lookup_region()` probes the override with
  `region_has_locale_data` (a CLDR region-display-name lookup) and keeps it
  only when that probe succeeds; otherwise it falls back to the resolved
  region. This keeps a valid override — including one absent from a
  call site's own literal data table — through to that table's world
  fallback, rather than abandoning it for the locale's ordinary region.
- `getHourCycles()` and `getCalendars()` both resolve through
  `lookup_region()` before consulting their own hour-cycle or calendar data.
- `getWeekInfo()` places that lookup region onto a cloned ICU locale before
  both week-data queries, keeping `firstDay` and `weekend` consistent.

## Scope

The helper is suitable for `getCalendars()`, but that method currently has no
region-sensitive calendar preference data to query. That independent data gap
is tracked in #562. `getHourCycles()` now resolves language-and-region-specific
CLDR time data before falling back to region-only data.

`getCollations()` does not use `RegionPreference`; its root fallback is fixed
separately to return the spec-required `emoji` and `eor` values.

## Validation

Pure helper tests cover alpha and numeric subdivision regions, malformed and
missing keywords, every region-preference level, and the `001` representation.
The public test262 Locale info tests cover `rg`, `sd`, likely-subtag, explicit
region, and world-region behavior through `getHourCycles()` and
`getWeekInfo()`.
