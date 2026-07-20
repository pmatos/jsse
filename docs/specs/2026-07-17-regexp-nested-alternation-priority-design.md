# RegExp nested-alternation priority design

## Context

PrismJS exposed three token-stream fixtures where nested quantified alternatives
selected a later, shorter match instead of the earliest greedy match required by
ECMAScript. The issue was filed against an earlier `main`; on the current pinned
`main`, a fresh release build already returns the same match, index, and capture
as Node for both reported minimal patterns.

The specification requires `RegExpBuiltinExec` to try input indices in ascending
order. At each index, a disjunction tries its left alternative before its right,
and a greedy `RepeatMatcher` tries another atom repetition before continuing with
the rest of the pattern.

## Requirements

- Permanently cover the two reported match-priority failures, including match
  start and captures where applicable.
- Run all three affected PrismJS fixtures instead of counting them as jsse-only
  skips.
- Preserve the existing standard regex backend unless a current failing case
  demonstrates that backend routing is still required.
- Keep the pinned PrismJS fixture count and Node cross-check unchanged.

## Approaches considered

1. Add the regressions and remove the obsolete PrismJS skips. This is the
   smallest change supported by the current engine behavior and converts the
   real-world reports into permanent coverage.
2. Detect quantified groups containing alternatives and force them through the
   fancy-regex VM with a zero-width assertion. This recreates ECMAScript's
   per-input-index search order, but adds parser-like detection and a slower
   matching path without a current failing test.
3. Force every standard pattern through the fancy-regex VM. This is simpler
   than selective detection but broadly changes RegExp performance and backend
   behavior beyond the issue's scope.

JSSE will use approach 1. If the unskipped PrismJS suite still fails, approach 2
is the bounded fallback.

## Design

Add a `test262-extra` test named for nested-alternation priority. It will cite
`RegExpBuiltinExec`, `CompileSubpattern`, and `RepeatMatcher`, then assert the
complete match, start index, and leading capture for the parenthesis pattern and
the complete match and start index for the Bison brace pattern.

Remove the three-entry `JSSE_SKIP_FIXTURES` map and its runtime guard from the
PrismJS generator so JSSE and Node execute the identical 2,563 fixtures. Update
the repository documentation to describe PrismJS as fully executed rather than
cross-checked with three engine-only skips.

No RegExp engine code will change unless the unskipped library suite supplies a
current counterexample.

## Validation

- Run the new `test262-extra` regression through the custom runner.
- Run the official RegExp test262 area, including the existing greedy
  `RepeatMatcher` cases.
- Run the complete pinned PrismJS suite and require 2,563 passes on both JSSE
  and Node.
- Run formatting, clippy, release build/tests, and the full test262 suite
  without updating the feature-branch baseline.
