// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-destructuring-binding-patterns
description: object binding pattern properties without a separating comma are a SyntaxError
info: |
  ArrayBindingPattern[Yield, Await] :
    [ Elision_opt BindingRestElement[?Yield, ?Await]_opt ]
    [ BindingElementList[?Yield, ?Await] ]
    [ BindingElementList[?Yield, ?Await] , Elision_opt BindingRestElement[?Yield, ?Await]_opt ]

  ObjectBindingPattern[Yield, Await] :
    { }
    { BindingRestProperty[?Yield, ?Await] }
    { BindingPropertyList[?Yield, ?Await] }
    { BindingPropertyList[?Yield, ?Await] , BindingRestProperty[?Yield, ?Await]_opt }

  Adjacent binding elements and properties must be separated by a comma.
negative:
  phase: parse
  type: SyntaxError
---*/

$DONOTEVALUATE();

var {a b} = {};
