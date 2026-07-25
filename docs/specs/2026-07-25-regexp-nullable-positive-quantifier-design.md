# Nullable positive RegExp quantifiers

## Problem

ECMAScript `RepeatMatcher` permits an atom to match empty while a quantifier's
minimum is still positive. Once the minimum reaches zero, another empty match
must fail so the atom can backtrack into a consuming choice before the outer
continuation is tried.

The Rust regex engines instead stop an unbounded nullable repetition when it
observes no input progress. For a pattern such as `(a*|dc??)+`, that accepts
the first branch's empty match and never tries the consuming sibling. The
existing source rewrite removes that empty path for `*`, but cannot do so
unconditionally for `+` or `{n,}` because their mandatory iterations may
legitimately be empty.

## Design

Keep one syntactic copy of every JavaScript capture. For each affected outer
quantifier with minimum `n > 0`, add `n` internal empty capture groups as
iteration sentinels:

1. During the first `n` iterations, a nullable branch retains its empty path.
2. A zero-width setter at the end of the quantified group sets one previously
   unset sentinel per completed iteration.
3. Once the final sentinel is set, the branch's empty path is replaced by a
   failing conditional, while the branch's existing consuming paths remain in
   their original alternative position.

For a bare nullable quantified atom, the consuming path is the existing
minimum-bumped rewrite (`a*` to `a+`, `a?` to `a`, or `{0,m}` to `{1,m}`).
The pre-minimum empty path is added alongside that consuming form, before it
for a lazy atom and after it for a greedy atom.

For a jointly optional sequence, the consuming expansion follows the original
greedy/lazy decision tree. Each greedy atom places its consuming choice before
the recursively expanded skip path; each lazy atom places it after. The gated
all-skipped leaf can therefore appear first, last, or between consuming
alternatives, preserving the source branch's mandatory-iteration priority.

Sentinel expansion is capped at 64 mandatory iterations. Larger minima retain
the existing fallback behavior instead of expanding source proportional to an
attacker-controlled quantifier bound. This preserves compilation behavior for
large valid quantifiers without introducing a source-size or compile-time
denial of service.

The sentinels use names with the existing `__jsse_qi` internal prefix. Their
definitions are appended in a `(?(DEFINE)...)` block after the translated
JavaScript pattern, while the repeated group invokes them with fancy-regex
subroutine calls. Defining the groups after all JavaScript groups keeps numeric
capture and backreference indices unchanged. Existing internal-capture cleanup
removes the sentinel slots before constructing the JavaScript match result.

The rewrite forces the fancy-regex path because the standard Rust regex engine
does not support the internal conditionals.

Capture-backed sentinels cannot be reset by fancy-regex after they participate.
An affected positive-minimum quantifier nested inside another repetition
therefore retains the previous non-stateful rewrite. This avoids leaking a
completed mandatory-iteration budget into the next entry of the nested
quantifier, while leaving the pre-existing nested priority gap unchanged.

Patterns that require JSSE's byte matcher (`\p{Cs}`/`\p{Co}` handling) retain
the previous non-stateful rewrite because that matcher cannot compile
fancy-regex conditionals. This avoids regressing valid byte-mode patterns while
leaving their positive-minimum priority gap unchanged.

## Alternatives

- Splitting `X+` into `X X*` duplicates captures and changes last-iteration
  capture semantics.
- Compiling a consuming pattern plus an all-empty fallback works for one
  affected group, but multiple groups require combinatorial variants and an
  additional match-priority algorithm.
- Restricting the split to noncapturing groups does not fix the reported
  capturing pattern.

## Validation

`test262-extra` coverage includes:

- the reported `+` result and last-iteration capture;
- legitimate empty `+` matches;
- `{2,}` and bounded `{2,3}` behavior;
- a continuation that requires falling back to the mandatory empty match;
- inner capture numbering and last-iteration values;
- lazy outer quantifiers; and
- multiple independently affected quantified groups;
- nested entry state isolation; and
- all-lazy and mixed-greediness jointly optional branches.

Validation runs the RegExp-focused test262 directory, the custom suites, and
the full test262 regression comparison before publishing.
