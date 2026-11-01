// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-patterns-static-semantics-early-errors
description: RegExp rejects a quantifier after a ^, $, \b or \B assertion and an unmatched ) in both modes
info: |
  Term[UnicodeMode, UnicodeSetsMode, NamedCaptureGroups] ::
    [~UnicodeMode] QuantifiableAssertion Quantifier
    Assertion[?UnicodeMode, ?UnicodeSetsMode, ?NamedCaptureGroups]
    Atom Quantifier

  ExtendedPatternCharacter excludes ) in Annex B, so an unmatched ) is a SyntaxError.
---*/

var flagSets = ["", "u"];
var quantified = ["^*", "^+", "^?", "^{2}", "$*", "$+", "\\b*", "\\b+", "\\B?", "\\B{1,2}", "a|^*", "(?:^)*x^*"];
var unmatched = [")", "a)", "(a))", "a|)"];

flagSets.forEach(function(flags) {
  quantified.concat(unmatched).forEach(function(source) {
    assert.throws(SyntaxError, function() {
      new RegExp(source, flags);
    }, "/" + source + "/" + flags);
  });
});

["(?:^)*", "(?:$)+", "(?:\\b)?", "(?=a)*", "(?!a)?", "^a*", "\\ba+", "a$|b^"].forEach(function(source) {
  new RegExp(source);
});
