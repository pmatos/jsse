// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-method-definitions
description: method parameters without a separating comma are a SyntaxError
info: |
  FormalParameters[Yield, Await] :
    [empty]
    FunctionRestParameter[?Yield, ?Await]
    FormalParameterList[?Yield, ?Await]
    FormalParameterList[?Yield, ?Await] ,
    FormalParameterList[?Yield, ?Await] , FunctionRestParameter[?Yield, ?Await]

  FormalParameterList[Yield, Await] :
    FormalParameter[?Yield, ?Await]
    FormalParameterList[?Yield, ?Await] , FormalParameter[?Yield, ?Await]
negative:
  phase: parse
  type: SyntaxError
---*/

$DONOTEVALUATE();

var o = { m(a b) {} };
