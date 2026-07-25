// Tests Annex B non-Unicode IdentityEscape parsing in character-class ranges.
// Spec: ECMAScript 2026, sec-regular-expressions-patterns and
// sec-additional-regular-expressions-patterns.

function assertSyntaxError(pattern) {
  try {
    new RegExp(pattern);
  } catch (error) {
    if (error instanceof SyntaxError) {
      return;
    }
    throw new Test262Error("expected SyntaxError, got " + error);
  }
  throw new Test262Error("expected invalid range to be rejected: " + pattern);
}

// Without Unicode mode, `\p` is an IdentityEscape for the character `p`.
// These are therefore the out-of-order ranges U+FFFF-p and `}`-`-`.
assertSyntaxError("[\uFFFF-\\p{Hex}]");
assertSyntaxError("[\\p{Hex}--]");

var identityEscape = new RegExp("[\\p{Any}]");
if (!identityEscape.test("p") || identityEscape.test("z")) {
  throw new Test262Error("non-Unicode \\p was not parsed as an identity escape");
}
