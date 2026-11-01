// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-grouping-operator
description: a trailing comma is only part of a parenthesized list that becomes arrow parameters
info: |
  PrimaryExpression :
    ParenthesizedExpression

  ParenthesizedExpression :
    ( Expression[+In, ?Yield, ?Await] )

  The Expression inside a ParenthesizedExpression has no trailing comma;
  `(a, )` is only valid as CoverParenthesizedExpressionAndArrowParameterList
  when it is followed by `=>`.
negative:
  phase: parse
  type: SyntaxError
---*/

$DONOTEVALUATE();

var x = (a, );
