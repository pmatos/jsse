// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-module-semantics-static-semantics-early-errors
description: a module-level let conflicts with a var declared in a nested block
info: |
  ModuleItemList : ModuleItemList ModuleItem

  It is a Syntax Error if any element of the LexicallyDeclaredNames of
  ModuleItemList also occurs in the VarDeclaredNames of ModuleItemList.
  VarDeclaredNames of a Statement includes the var declarations nested in its
  blocks, loops, if statements, switch statements, labelled statements and
  try statements.
negative:
  phase: parse
  type: SyntaxError
flags: [module]
---*/

$DONOTEVALUATE();

let x;
{ var x; }
