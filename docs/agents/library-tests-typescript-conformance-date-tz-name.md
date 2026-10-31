# Date time zone name and Intl zone-name separator (#1480)

Part of the TypeScript epic (#1214). ESLint's html formatter prints
`new Date().toString()`, so two host-visible strings differed between Node and
`jsse --node`.

## What changed

1. `Date.prototype.toString`, `toTimeString` and `Date()` print the CLDR long
   name of the zone in the parentheses under `--node` (`GMT+0100 (Central
   European Standard Time)`, like V8). ECMA-262 TimeZoneString leaves the text
   implementation-defined, so the default environment keeps the tzdata
   abbreviation (`(CET)`). The long name comes from ICU4X
   (`NoCalendarFormatter<SpecificLong>`, locale `en`) in
   `src/interpreter/helpers/tz_long_name.rs`, the same data
   `Intl.DateTimeFormat` `timeZoneName: "long"` uses; the hand-written
   abbreviation table `tz_abbr_to_long_name` is gone and the raw fallback
   formatter shares `long_time_zone_name`.
2. A zone name that follows date fields with no time field is joined like a
   date-time (`1/1/1970, Central European Standard Time`;
   `January 1 at Central European Standard Time` after a long month), in both
   modes. ICU4X joins date and zone with a plain space; ICU4C (V8) uses the
   date-time glue chosen by month width (MMMM long, with a weekday full, MMM
   medium, otherwise short). `zone_glue.rs` under
   `src/interpreter/builtins/intl/datetimeformat/` reads that glue off a probe
   formatted with an hour instead of the zone, in the same locale.

## Tests

- `test262-extra/Intl-DateTimeFormat-date-zone-name-separator-1480.js`
  (explicit `timeZone`, so independent of `TZ`).
- `tests/node_date_tz_long_name_1480.rs` pins `TZ` (Berlin, New York, UTC,
  Kolkata) and checks `--node` against Node 26 strings, plus the default mode.
- Compare by hand: `TZ=Europe/Berlin node -p 'String(new Date(0))'` against
  `TZ=Europe/Berlin jsse --node -e 'console.log(String(new Date(0)))'`.

## Known remaining differences from V8

- V8 names `Date.prototype.toString` by the zone's DST flag and its current
  metazone, not by the instant: for eras whose offset is not the zone's
  standard or daylight offset (Europe/London 1970, British Standard Time) V8
  prints `(Greenwich Mean Time)`; jsse prints the ICU4X offset form
  `(GMT+01:00)`, which is what V8's `Intl.DateTimeFormat` prints too.
  Europe/Dublin in 1970 likewise prints `Irish Standard Time` against V8's
  `Greenwich Mean Time` in `Date.prototype.toString`.
- V8 formats `Intl.DateTimeFormat` with a plain space before `AM`/`PM` where
  jsse (ICU4X, CLDR 42+) prints U+202F; unrelated to this issue and left as is.
- Option sets ICU4X cannot express (year-only, month-only or era with a zone)
  still use the English-only fallback formatter in other locales, and its
  `timeZoneName: "short"` prints the tzdata abbreviation (`CET`) where V8
  prints `GMT+1`. The fallback now uses the same separators for `en`.
- Locale data differences between ICU4X and ICU4C (digit padding in numeric
  dates for `de`, `ja`, `ko`; Thai and Persian calendars) also show in the
  date-plus-zone strings and are not zone-name issues.
