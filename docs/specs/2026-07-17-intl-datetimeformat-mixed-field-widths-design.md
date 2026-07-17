# Independent textual widths in `Intl.DateTimeFormat`

## Problem

The ICU4X 2.x semantic `FieldSetBuilder` accepts one `Length` for an entire
date field set. An ECMA-402 request can independently select `long`, `short`,
or `narrow` for `weekday`, `era`, and textual `month`, so reducing those
requests to one length changes observable field values. Falling back to JSSE's
hand-written formatter preserves widths only in English and therefore is not a
locale-correct solution.

ECMA-262 delegates the locale-sensitive date methods to ECMA-402. ECMA-402's
date-time format record retains each requested component independently, while
the selected locale pattern controls ordering and literals. `format()` and
`formatToParts()` must be produced from the same selected pattern.

## Design

Use ICU4X's classical component-skeleton matcher as a narrow compatibility
seam for explicit component sets whose requested textual widths differ:

- Translate the resolved `DtfOptions` components into ICU classical fields,
  preserving each requested numeric and textual width.
- Load the locale's classical skeleton and date/time glue data through the
  ICU4X 1.5 compiled-data surface, which still exposes component-based pattern
  selection. Accept only an all-fields match; otherwise retain the existing
  fallback behavior.
- Serialize the selected UTS 35 pattern and parse it with ICU4X 2.x.
- Use ICU4X 2.x `FixedCalendarDateTimeNames` to load the names required by that
  pattern and format the already time-zone-adjusted Gregorian input. Collect
  its annotated output through the existing parts writer and normalization.
- Keep the current ICU4X 2.x semantic field-set path unchanged for uniform
  widths, styles, and other requests.

The old ICU surface chooses only the pattern. ICU4X 2.x remains responsible for
localized symbols, numbering, time-zone names, and annotated parts, avoiding
two competing output implementations.

## Alternatives

Formatting the composite field set several times and substituting individual
values is smaller, but it cannot repair literals selected for the wrong global
length. For example, Japanese commonly wraps abbreviated weekdays in
parentheses but not long weekdays.

Rewriting widths in an ICU4X 2.x semantic pattern has the same structural
problem: the starting pattern was selected from one global length and can have
the wrong punctuation, padding, or ordering for the mixed skeleton.

Routing mixed widths to the existing JSSE formatter restores English widths
but emits English names for non-English locales, directly regressing locale
correctness.

## Validation

The public seam is the JavaScript `Intl.DateTimeFormat` API. Repository-owned
tests will compare literal `format()` results for mixed long/short/narrow
weekday, era, and month widths in representative English, French, Japanese,
and Russian locales. They will also assert that `formatToParts()` exposes each
requested value and concatenates to `format()`.

The targeted DateTimeFormat test262 directory and the full test262 suite remain
regression gates. Since test262 does not assert locale-dependent byte output,
the new `test262-extra` coverage is the primary regression test.
