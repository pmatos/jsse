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
- `getHourCycles()` uses the override when present, otherwise the resolved
  region, before consulting its existing hour-cycle data.
- `getWeekInfo()` places that lookup region onto a cloned ICU locale before
  both week-data queries, keeping `firstDay` and `weekend` consistent.

The existing data providers return a world fallback for syntactically valid
regions, so a valid `rg` value always has usable lookup data in these two
paths. There is no separate availability probe before selecting the override.

## Scope

The helper is suitable for `getCalendars()`, but that method currently has no
region-sensitive calendar preference data to query. That independent data gap
is tracked in #562. Language-and-region-specific hour-cycle preferences are
also absent from the existing region-only table and are tracked in #563.

`getCollations()` does not use `RegionPreference`; its root fallback is fixed
separately to return the spec-required `emoji` and `eor` values.

## Validation

Pure helper tests cover alpha and numeric subdivision regions, malformed and
missing keywords, every region-preference level, and the `001` representation.
The public test262 Locale info tests cover `rg`, `sd`, likely-subtag, explicit
region, and world-region behavior through `getHourCycles()` and
`getWeekInfo()`.
