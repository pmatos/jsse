// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-identifiers-static-semantics-early-errors
description: >
  `yield` written with a unicode escape is a reserved word as a shorthand property in a generator
info: |
  IdentifierReference[Yield, Await] :
    Identifier
    [~Yield] yield

  It is a Syntax Error if this production has a [Yield] parameter and StringValue of Identifier is
  "yield".
negative:
  phase: parse
  type: SyntaxError
---*/

$DONOTEVALUATE();

function* g() {
  ({ yi\u0065ld });
}
