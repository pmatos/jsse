// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-variable-statement
description: var array binding pattern without an initializer is a SyntaxError
info: |
  VariableDeclaration[In, Yield, Await] :
    BindingIdentifier Initializer[?In, ?Yield, ?Await]opt
    BindingPattern[?Yield, ?Await] Initializer[?In, ?Yield, ?Await]

  A BindingPattern is only a VariableDeclaration together with an Initializer.
negative:
  phase: parse
  type: SyntaxError
---*/

$DONOTEVALUATE();

var [a];
