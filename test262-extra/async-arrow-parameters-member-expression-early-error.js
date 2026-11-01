// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-async-arrow-function-definitions-static-semantics-early-errors
description: a member expression is not a binding element of async arrow parameters
info: |
  AsyncArrowHead :
    async [no LineTerminator here] ArrowFormalParameters[~Yield, +Await]

  The CoverCallExpressionAndAsyncArrowHead is refined to the AsyncArrowHead,
  whose elements are BindingElements.
negative:
  phase: parse
  type: SyntaxError
features: [async-functions]
---*/

$DONOTEVALUATE();

async (a.b) => 1;
