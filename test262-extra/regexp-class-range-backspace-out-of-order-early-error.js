// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-patterns-static-semantics-early-errors
description: \b is U+0008, so a range from \b to U+0001 is out of order
info: |
  ClassRanges, NonemptyClassRanges[UnicodeMode] :
    ClassAtom - ClassAtom ClassContents

  It is a Syntax Error if IsCharacterClass of the first ClassAtom is true or
  IsCharacterClass of the second ClassAtom is true and this production has a
  [UnicodeMode] parameter (22.2.1.1). `\b` in a class is a CharacterEscape,
  not a CharacterClassEscape, so a range ending in `\d` is invalid with u.
negative:
  phase: parse
  type: SyntaxError
---*/

$DONOTEVALUATE();

/[\b-\x01]/;
