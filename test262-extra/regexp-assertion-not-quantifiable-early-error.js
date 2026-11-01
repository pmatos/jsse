// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-patterns-static-semantics-early-errors
description: a quantifier after a ^, $, \b or \B assertion is a SyntaxError in a literal
info: |
  Term[UnicodeMode, UnicodeSetsMode, NamedCaptureGroups] ::
    [~UnicodeMode] QuantifiableAssertion Quantifier
    Assertion[?UnicodeMode, ?UnicodeSetsMode, ?NamedCaptureGroups]
    Atom Quantifier

  Assertion :: ^ | $ | \b | \B
  Only the lookahead assertions of Annex B are QuantifiableAssertions.
negative:
  phase: parse
  type: SyntaxError
---*/

$DONOTEVALUATE();

/^*/;
