# Date offsets across the TimeClip range

## Context

`Date` now resolves the host IANA time zone through `chrono-tz`, but chrono
cannot construct datetimes for every ECMAScript time value. The current
fallback returns a zero offset when a value is outside chrono's year range.
That makes valid boundary dates behave as UTC even when `TZ` names another
zone. In addition, chrono-tz 0.10's generated transitions end after 2099, so
using a later chrono datetime directly loses recurring daylight-saving rules.

ECMAScript `LocalTime` and `UTC` require the selected IANA zone's political
rules across the full TimeClip range. `UTC` must also accept nearby nominal
local values outside that range when applying an offset brings the result back
inside it.

## Design

Introduce one conversion seam that returns a chrono proxy datetime for an
ECMAScript time value:

- Values through 2099 that chrono can represent use their exact datetime.
- Later values use the same month, day, and time in the latest year from
  2072–2099 with the same leap-year status and weekday for January 1. That
  28-year window contains every Gregorian calendar shape and chrono-tz's final
  generated recurring rules, so month/day and weekday-based transitions remain
  aligned.
- Values earlier than chrono can represent use chrono's earliest datetime.
  They therefore select the first offset in the IANA zone history, matching the
  zone's behavior before its first recorded transition.

UTC-to-local offset lookup, local-to-UTC offset lookup, and time-zone
abbreviation lookup all use this seam. Local-time gaps and overlaps retain the
existing ECMAScript-compatible disambiguation: choose the offset before the
transition.

## Alternatives

Parsing TZif files and POSIX tail rules locally would avoid proxy dates, but
would add a substantial parser and platform-specific data lookup. Adding a
second timezone library would duplicate chrono-tz and widen the dependency
surface. The proxy approach is smaller and uses the IANA data already selected
by the engine.

## Verification

The host-time-zone integration test will assert:

- winter and summer offsets in an ordinary year;
- recurring daylight-saving behavior in 2100;
- upper-boundary local construction clipping to
  `+275760-09-13T00:00:00.000Z` in `America/New_York`;
- lower-boundary local construction using New York's first historical offset;
- nonzero local offsets and formatting at both boundaries;
- default Intl formatting using the recurring offset at the upper boundary;
- existing daylight-saving gap, overlap, and cross-transition behavior.

The full Date test262 subtree and full default test262 suite will guard the
existing UTC-pinned conformance behavior.
