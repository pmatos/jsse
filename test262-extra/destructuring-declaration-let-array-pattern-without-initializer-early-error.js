// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-let-and-const-declarations
description: let array binding pattern without an initializer is a SyntaxError
info: |
  LexicalBinding[In, Yield, Await] :
    BindingIdentifier[?Yield, ?Await] Initializer[?In, ?Yield, ?Await]opt
    BindingPattern[?Yield, ?Await] Initializer[?In, ?Yield, ?Await]

  A BindingPattern is only a LexicalBinding together with an Initializer.
negative:
  phase: parse
  type: SyntaxError
---*/

$DONOTEVALUATE();

let [a];
