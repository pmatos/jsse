// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-destructuring-assignment-static-semantics-early-errors
description: the rest property of an object assignment pattern must not be an object literal
info: |
  AssignmentRestProperty[Yield, Await] :
    ... DestructuringAssignmentTarget[?Yield, ?Await]

  It is a Syntax Error if DestructuringAssignmentTarget is an ArrayLiteral or
  an ObjectLiteral.
negative:
  phase: parse
  type: SyntaxError
---*/

$DONOTEVALUATE();

({...{a}} = {});
