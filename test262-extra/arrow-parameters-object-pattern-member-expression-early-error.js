// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-arrow-function-definitions-static-semantics-early-errors
description: a member expression as an object pattern value is not a binding element of arrow parameters
info: |
  ArrowFunction[In, Yield, Await] :
    ArrowParameters[?Yield, ?Await] [no LineTerminator here] => ConciseBody[?In]

  ArrowParameters[Yield, Await] :
    BindingIdentifier[?Yield, ?Await]
    CoverParenthesizedExpressionAndArrowParameterList[?Yield, ?Await]

  When a CoverParenthesizedExpressionAndArrowParameterList is recognized as
  ArrowParameters, its contents are reparsed as
  ArrowFormalParameters[?Yield, ?Await] : ( UniqueFormalParameters ), whose
  elements are BindingElements: a MemberExpression, a parenthesized binding
  and an object or array literal under an object rest are not.
negative:
  phase: parse
  type: SyntaxError
---*/

$DONOTEVALUATE();

({a: b.c}) => 1;
