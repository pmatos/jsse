// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-try-statement
description: a destructured catch parameter conflicts with a for-in var binding of the same name
info: |
  Catch : catch ( CatchParameter ) Block

  It is a Syntax Error if any element of the BoundNames of CatchParameter also
  occurs in the VarDeclaredNames of Block, unless CatchParameter is
  CatchParameter : BindingIdentifier (B.3.4 VariableStatements in Catch Blocks).
negative:
  phase: parse
  type: SyntaxError
---*/

$DONOTEVALUATE();

try {} catch ([e]) { for (var e in []); }
