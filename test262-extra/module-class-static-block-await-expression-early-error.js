// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-class-definitions-static-semantics-early-errors
description: await expression directly in a class static block is a SyntaxError in module code
info: |
  ClassStaticBlockBody : ClassStaticBlockStatementList

  It is a Syntax Error if ContainsAwait of ClassStaticBlockStatementList is true.
flags: [module]
negative:
  phase: parse
  type: SyntaxError
---*/

$DONOTEVALUATE();

class C {
  static {
    await 1;
  }
}
