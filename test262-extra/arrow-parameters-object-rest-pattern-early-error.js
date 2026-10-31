// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-arrow-function-definitions-static-semantics-early-errors
description: the rest of an object pattern in arrow parameters must be a BindingIdentifier
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

  BindingRestProperty[Yield, Await] :
    ... BindingIdentifier[?Yield, ?Await]
negative:
  phase: parse
  type: SyntaxError
---*/

$DONOTEVALUATE();

({...{a}}) => 1;
