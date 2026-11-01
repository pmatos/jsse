// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-patterns-static-semantics-early-errors
description: an unmatched ) in a regular expression literal is a SyntaxError, also without the u flag
info: |
  ExtendedAtom[NamedCaptureGroups] ::
    ...
    ExtendedPatternCharacter

  ExtendedPatternCharacter :: SourceCharacter but not one of ^ $ \ . * + ? ( ) [ |
negative:
  phase: parse
  type: SyntaxError
---*/

$DONOTEVALUATE();

/a)/;
