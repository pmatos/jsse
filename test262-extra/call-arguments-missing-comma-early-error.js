// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-argument-lists
description: call arguments without a separating comma are a SyntaxError
info: |
  Arguments[Yield, Await] :
    ( )
    ( ArgumentList[?Yield, ?Await] )
    ( ArgumentList[?Yield, ?Await] , )

  ArgumentList[Yield, Await] :
    AssignmentExpression[+In, ?Yield, ?Await]
    ... AssignmentExpression[+In, ?Yield, ?Await]
    ArgumentList[?Yield, ?Await] , AssignmentExpression[+In, ?Yield, ?Await]
    ArgumentList[?Yield, ?Await] , ... AssignmentExpression[+In, ?Yield, ?Await]

  Adjacent arguments must be separated by a comma.
negative:
  phase: parse
  type: SyntaxError
---*/

$DONOTEVALUATE();

f(1 2);
