// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-for-in-and-for-of-statements
description: the left-hand side of a for-of loop cannot start with the token let, also when a member access follows
info: |
  IterationStatement[Yield, Await, Return] :
    for ( [lookahead ∉ { let, async of }] LeftHandSideExpression[?Yield, ?Await] of AssignmentExpression[+In, ?Yield, ?Await] ) Statement[?Yield, ?Await, ?Return]
negative:
  phase: parse
  type: SyntaxError
---*/

$DONOTEVALUATE();

for (let.x of []) ;
