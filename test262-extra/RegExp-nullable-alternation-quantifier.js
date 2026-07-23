// Tests spec §22.2.2.6.1 RepeatMatcher step 2.b for a repeated group whose
// top-level alternation mixes a nullable branch (one that can match the
// empty string, e.g. `a*`) with a non-nullable sibling branch. An iteration
// that matches empty must be discarded — but a backtracking-free engine can
// otherwise commit to the nullable branch's empty match before ever trying a
// sibling branch able to consume input at that position, dropping matched
// text (jsse#370).
//
// This is a distinct failure mode from the single-alternative nullable body
// already covered by test262's built-ins/RegExp/nullable-quantifier.js
// (`/(a?b??)*/`): there, no alternation is involved, so a lazy sub-quantifier
// forced greedy is sufficient. Here the fix must not touch a non-nullable
// sibling branch's own laziness, and must not reorder alternatives (which
// would change which branch wins when both can consume — see the `ab`
// checks below).

// The reported repro: outer `*` group alternates a nullable `a*` with a
// non-nullable, lazily-quantified `dc??`.
assert.sameValue(/(a*|dc??)*/.exec("dc")[0], "d");
assert.sameValue(/(a*|dc??)*/.exec("dcc")[0], "d");

// Bounded (`{0,n}`) and unbounded (`{0,}`) nullable branches must be handled
// the same way as `*`.
assert.sameValue(/(a{0,2}|dc??)*/.exec("dc")[0], "d");
assert.sameValue(/(a{0,}|dc??)*/.exec("dc")[0], "d");

// The same root cause reproduces without any lazy quantifier at all — a
// plain nullable branch alongside a plain mandatory-literal sibling.
assert.sameValue(/(a*|bc)*/.exec("bc")[0], "bc");
assert.sameValue(/(a*|bc)*/.exec("abc")[0], "abc");

// The fix must not reorder alternatives: when the nullable branch itself can
// match non-empty text that a later sibling could also match, the leftmost
// (nullable) branch's own non-empty match still wins, exactly as today.
assert.sameValue(/(a*|ab)*/.exec("ab")[0], "a");
assert.sameValue(/(a+|ab)*/.exec("ab")[0], "a");

// Capture group numbering must be unaffected — the fix rewrites a branch's
// own quantifier in place, it never moves parentheses.
var m = /((a)*|(dc)??)*/.exec("dc");
assert.sameValue(m[0], "dc");
assert.sameValue(m[2], undefined);
assert.sameValue(m[3], "dc");
