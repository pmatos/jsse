// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-grouping-operator
description: a rest element in a parenthesized list is only valid when it becomes arrow parameters
info: |
  CoverParenthesizedExpressionAndArrowParameterList :
    ( Expression , ... BindingIdentifier )

  This production may only be used as ArrowParameters; `(a, ...b)` that is
  not followed by `=>` is a SyntaxError.
negative:
  phase: parse
  type: SyntaxError
---*/

$DONOTEVALUATE();

var x = (a, ...b);
