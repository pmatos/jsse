// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-arrow-function-definitions
description: no LineTerminator may separate a rest-only arrow parameter list from the arrow
info: |
  ArrowFunction[In, Yield, Await] :
    ArrowParameters[?Yield, ?Await] [no LineTerminator here] => ConciseBody[?In]
negative:
  phase: parse
  type: SyntaxError
---*/

$DONOTEVALUATE();

var f = (...a)
=> 1;
