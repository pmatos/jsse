// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-for-in-and-for-of-statements
description: a for-of head with several declarators still needs an initializer on each pattern
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

for (var {a}, b of []) { }
