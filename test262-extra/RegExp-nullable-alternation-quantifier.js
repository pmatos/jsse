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

// The bump must not apply under `+` (min=1): the *required* first iteration
// of a `+`-quantified group is still allowed to match empty per spec — only
// iterations after it are discarded if empty — so a nullable branch (`a*`,
// or its lazy form `a*?`) must stay able to satisfy that first iteration.
// Forbidding the empty case there (as an earlier draft of this fix did) is
// its own regression, distinct from the `*` case above.
assert.sameValue(/(a*|b)+/.exec("")[0], "");
assert.sameValue(/(a*|b)+/.exec("")[1], "");
assert.sameValue(/(a*?|b)+/.exec("")[0], "");
assert.sameValue(/(a*?|b)+/.exec("")[1], "");

// --- Residual gaps closed (jsse#373) ---
// A few other shapes are nullable through mechanisms the bump above doesn't
// cover: a bare empty alternative, a nullable atom with no quantifier suffix
// of its own, several jointly-optional atoms, and an exact-zero quantifier.
// All four pre-existed the #370 fix (confirmed via a Node-diff matrix, not
// regressions) and are fixed here.

// Shape 1: a bare empty alternative branch (`(|a)*`, `(a|)*`) has nothing to
// bump — it's spliced out of the alternation entirely instead.
assert.sameValue(/(|a)*/.exec("a")[0], "a");
assert.sameValue(/(a|)*/.exec("a")[0], "a");
assert.sameValue(/(|a|b)*/.exec("ba")[0], "ba");

// Shape 2: several jointly-optional atoms (`a?b?`) are nullable only because
// every atom is individually optional — bumping any single one would wrongly
// reject strings where only the others are present, so the whole sequence is
// expanded into an alternation requiring the first-consuming atom.
assert.sameValue(/(a?b?|dc??)*/.exec("dc")[0], "d");
assert.sameValue(/(a?b?|dc??)*/.exec("ab")[0], "ab");
assert.sameValue(/(a?b?|dc??)*/.exec("ba")[0], "ba");
assert.sameValue(/(a*b?c{0,2}|dc??)*/.exec("aabcc")[0], "aabcc");
// A capturing group among the jointly-optional atoms must block the
// rewrite (it would otherwise renumber later capture groups) — this stays
// the pre-#373 (unfixed) result, not a regression.
assert.sameValue(/((a)?(b)?|dc??)*/.exec("ab")[0], "ab");

// Shape 3: a bare group atom with no quantifier of its own (`(a*)`) is
// nullable through its own interior, not a suffix quantifier — recognized by
// recursing nullability/fix logic into the group's own alternation.
assert.sameValue(/(?:(a*)|(?:dc)??)*/.exec("dc")[0], "dc");
assert.sameValue(/(?:((a*))|(?:dc)??)*/.exec("dc")[0], "dc");

// Shape 4: an exact-zero quantifier (`a{0}`, `a{0,0}`) always matches empty
// — bumping the floor to 1 with max already 0 would produce an invalid
// quantifier, so it's spliced out like shape 1 instead.
assert.sameValue(/(a{0}|dc??)*/.exec("dc")[0], "d");
assert.sameValue(/(a{0,0}|dc??)*/.exec("dc")[0], "d");

// Capture-group numbering must be unaffected by the splice: deleting the
// non-capturing `a{0}` branch must not disturb the sibling's own capture.
var m2 = /(a{0}|(dc)??)*/.exec("dc");
assert.sameValue(m2[0], "dc");
assert.sameValue(m2[1], "dc");

// --- Positive-minimum outer quantifiers (jsse#378) ---
// The first `min` iterations may match empty. Once `min` reaches zero, another
// empty iteration must fail and backtrack into a consuming sibling before the
// outer continuation is accepted.
var plus = /(a*|dc??)+/.exec("dc");
assert.sameValue(plus[0], "d");
assert.sameValue(plus[1], "d");

// The last consuming iteration still owns the capture; rewriting `X+` as
// `X X*` would incorrectly leave the mandatory copy's capture in a new slot.
var plusLast = /(a*|dc??)+/.exec("aaadc");
assert.sameValue(plusLast[0], "aaad");
assert.sameValue(plusLast[1], "d");

// More than one mandatory empty iteration is permitted for `{n,}` and bounded
// `{n,m}` forms. The consuming sibling is tried only in the optional
// post-minimum iteration.
var atLeastTwo = /(a*|d){2,}/.exec("d");
assert.sameValue(atLeastTwo[0], "d");
assert.sameValue(atLeastTwo[1], "d");
var twoToThree = /(a*|d){2,3}/.exec("d");
assert.sameValue(twoToThree[0], "d");
assert.sameValue(twoToThree[1], "d");

// An exact quantifier has no post-minimum iteration. Its left nullable branch
// therefore remains the correct winner.
var exactlyTwo = /(a*|d){2}/.exec("d");
assert.sameValue(exactlyTwo[0], "");
assert.sameValue(exactlyTwo[1], "");

// If consuming the sibling makes the sequel fail, RepeatMatcher must still
// backtrack to the mandatory empty iteration and try the continuation there.
var continuationFallback = /(a*|d)+d/.exec("d");
assert.sameValue(continuationFallback[0], "d");
assert.sameValue(continuationFallback[1], "");

// Inner capture numbering and last-iteration values must remain unchanged.
var innerCaptures = /((a)*|(dc)??)+/.exec("dc");
assert.sameValue(innerCaptures[0], "dc");
assert.sameValue(innerCaptures[1], "dc");
assert.sameValue(innerCaptures[2], undefined);
assert.sameValue(innerCaptures[3], "dc");

// Internal iteration state is defined after all JavaScript captures, so later
// numeric slots and backreferences retain their source-level numbering.
var laterBackref = /(a*|d)+(e)\2/.exec("dee");
assert.sameValue(laterBackref[0], "dee");
assert.sameValue(laterBackref[1], "d");
assert.sameValue(laterBackref[2], "e");
var namedBackref = /(?<x>a*|d)+\k<x>/.exec("dd");
assert.sameValue(namedBackref[0], "dd");
assert.sameValue(namedBackref.groups.x, "d");

// A lazy outer quantifier still accepts its mandatory empty match when the
// continuation succeeds, and consumes a sibling only when the continuation
// forces another iteration.
assert.sameValue(/(a*|d)+?/.exec("d")[0], "");
var lazyInnerAndOuter = /(a*?|d)+?/.exec("a");
assert.sameValue(lazyInnerAndOuter[0], "");
assert.sameValue(lazyInnerAndOuter[1], "");
var lazyForced = /(a*|d)+?c/.exec("dc");
assert.sameValue(lazyForced[0], "dc");
assert.sameValue(lazyForced[1], "d");

// The positive-minimum treatment also covers the residual nullable shapes
// handled for `*`: bare empty, exact-zero, and jointly-optional branches.
assert.sameValue(/(|d)+/.exec("d")[0], "d");
assert.sameValue(/(a{0}|d)+/.exec("d")[0], "d");
assert.sameValue(/(a?b?|d)+/.exec("d")[0], "d");

// Separate positive-minimum groups keep independent iteration state. Either
// group may legitimately finish with only its mandatory empty iteration.
var sequentialOne = /^(a*|d)+(b*|e)+/.exec("d");
assert.sameValue(sequentialOne[0], "d");
assert.sameValue(sequentialOne[1], "d");
assert.sameValue(sequentialOne[2], "");
var sequentialBoth = /^(a*|d)+(b*|e)+/.exec("de");
assert.sameValue(sequentialBoth[0], "de");
assert.sameValue(sequentialBoth[1], "d");
assert.sameValue(sequentialBoth[2], "e");
