// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-optional-chaining
description: optional call arguments without a separating comma are a SyntaxError
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

var f;
f?.(1 2);
