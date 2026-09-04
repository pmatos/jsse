// Tests the Annex B ExtendedPatternCharacter fallback: in non-Unicode mode, a
// `{` that does not open a syntactically valid quantifier (`{n}`, `{n,}`,
// `{n,m}`, each requiring at least one leading digit) is an ordinary literal
// character rather than a syntax error.
// Spec: ECMAScript 2026, sec-additional-regular-expressions-patterns
// (Pattern[~U, ~N] :: ExtendedTerm and InvalidBracedQuantifier).

function assertLiteralSource(pattern, flags) {
  var re = new RegExp(pattern, flags);
  if (re.source !== pattern) {
    throw new Test262Error(
      "expected literal pattern to round-trip: " + pattern + " got " + re.source
    );
  }
}

function assertSyntaxError(pattern, flags) {
  try {
    new RegExp(pattern, flags);
  } catch (error) {
    if (error instanceof SyntaxError) {
      return;
    }
    throw new Test262Error("expected SyntaxError, got " + error);
  }
  throw new Test262Error("expected invalid quantifier to be rejected: " + pattern);
}

// Braces with no digits at all: not a quantifier, fall back to literal.
assertLiteralSource("{}", "g");
assertLiteralSource("a{}", "");
assertLiteralSource("{,}", "");
assertLiteralSource("{,5}", "");

// More than one comma is not valid QuantifierPrefix grammar either.
assertLiteralSource("{1,2,3}", "");
assertLiteralSource("a{1,2,3}", "");

// A literal brace pair directly following a real quantifier must not be
// misread as a second (invalid) quantifier on the same atom.
assertLiteralSource("a*{}", "");
assertLiteralSource("a+{}", "");
assertLiteralSource("a?{}", "");
assertLiteralSource("a{1}{}", "");

// Lookaround assertions followed by a non-quantifier brace pair.
assertLiteralSource("(?=a){}", "");
assertLiteralSource("(?<=a){}", "");
assertLiteralSource("(?<!a){}", "");

// Genuine quantifiers (at least one leading digit) with no preceding atom
// remain syntax errors — Annex B's InvalidBracedQuantifier still applies.
assertSyntaxError("{1}", "");
assertSyntaxError("{1,}", "");
assertSyntaxError("{1,2}", "");

// The `u`/`v` flags are unaffected: braces are always subject to the
// (stricter) Unicode-mode quantifier grammar there.
assertSyntaxError("{}", "u");
assertSyntaxError("a{}", "u");
assertSyntaxError("a*{}", "u");

// Matching semantics: a literal `{}` must match itself, not just parse.
if (!/{}/.test("{}")) {
  throw new Test262Error("literal {} pattern failed to match its own text");
}
var match = /a{}b/.exec("a{}b");
if (!match || match[0] !== "a{}b") {
  throw new Test262Error("literal {} between atoms failed to match");
}

// Comma-leading brace forms (`{,}`, `{,n}`) are the ones a host regex engine
// is most likely to mistake for a lenient real quantifier (many non-ECMA
// flavors treat `{,n}` as shorthand for `{0,n}`). Assert actual matched text
// via exec(), not just whether the pattern parses, in every context that
// resets `has_atom`: no atom, a plain atom, after a real quantifier, and
// after each lookaround assertion kind.
function assertExecMatch(pattern, flags, str, expected) {
  var re = new RegExp(pattern, flags);
  var m = re.exec(str);
  var actual = m ? m[0] : null;
  if (actual !== expected) {
    throw new Test262Error(
      "/" + pattern + "/" + flags + " against " + JSON.stringify(str) +
      ": expected " + JSON.stringify(expected) + ", got " + JSON.stringify(actual)
    );
  }
}

assertExecMatch("{,}", "", "{,}", "{,}");
assertExecMatch("{,5}", "", "{,5}", "{,5}");
assertExecMatch("a{,}", "", "a", null);
assertExecMatch("a{,}", "", "a{,}", "a{,}");
assertExecMatch("a*{,}", "", "aaa", null);
assertExecMatch("a*{,}", "", "aaa{,}", "aaa{,}");
assertExecMatch("(?=.){,}", "", "{,}", "{,}");
assertExecMatch("(?=.){,}", "", "x", null);
assertExecMatch("(?=a){,}", "", "a", null);
assertExecMatch("(?<=a){,}", "", "a", null);
assertExecMatch("(?<=a){,}", "", "a{,}", "{,}");
assertExecMatch("(?<=a){,5}", "", "a", null);
assertExecMatch("(?<=(a)){,}", "", "a", null);
assertExecMatch("(?<=(a)){,}", "", "a{,}", "{,}");

// Real quantifiers must be unaffected by the escaping added for the literal
// fallback (they still consume the braces as an actual quantifier, not as
// literal text).
assertExecMatch("a{2,3}", "", "aaaa", "aaa");
assertExecMatch("a{2}", "", "aaaa", "aa");
assertExecMatch("a{2,}", "", "aaaa", "aaaa");

// `{0,foo}` has a leading digit ("0") but is still not a valid quantifier,
// since the grammar requires DecimalDigits on both sides of the comma. Two
// internal passes independently reason about brace quantifiers by peeking at
// the leading digit alone, and both must be gated the same way as the main
// translator, or they mistake this literal for a genuine {0,...} minimum:
// - the nullable/lazy-marker rewrite, which can drop a real `??` elsewhere in
//   the same repeated group if it thinks the group can match empty;
// - the quantified-group capture tracker, which clears a nested capture
//   between "iterations" that don't actually exist.
function assertExecGroups(pattern, flags, str, expectedGroups) {
  var re = new RegExp(pattern, flags);
  var m = re.exec(str);
  var actual = m ? Array.prototype.slice.call(m) : null;
  if (JSON.stringify(actual) !== JSON.stringify(expectedGroups)) {
    throw new Test262Error(
      "/" + pattern + "/" + flags + " against " + JSON.stringify(str) +
      ": expected " + JSON.stringify(expectedGroups) + ", got " + JSON.stringify(actual)
    );
  }
}

assertExecMatch("(a{0,foo}b??)*", "", "a{0,foo}b", "a{0,foo}");
assertExecGroups("(?:(?=(.))){0,foo}", "", "{0,foo}", ["{0,foo}", "{"]);
assertExecGroups(
  "((?:(?=(.))){0,foo}x)*", "", "{0,foo}x{0,foo}x",
  ["{0,foo}x{0,foo}x", "{0,foo}x", "{"]
);
assertExecGroups("(a){0,foo}", "", "a{0,foo}", ["a{0,foo}", "a"]);
