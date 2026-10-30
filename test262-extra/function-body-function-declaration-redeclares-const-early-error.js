// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-function-definitions-static-semantics-early-errors
description: a top-level function declaration in a function body may not share a name with a const of the same body
info: |
  FunctionBody : FunctionStatementList

  It is a Syntax Error if the LexicallyDeclaredNames of FunctionStatementList
  contains any duplicate entries.

  It is a Syntax Error if any element of the LexicallyDeclaredNames of
  FunctionStatementList also occurs in the VarDeclaredNames of
  FunctionStatementList.

  8.2.5 Static Semantics: TopLevelVarDeclaredNames
  FunctionStatementList's top-level function declarations are var-scoped, so
  `function a() {}` is a VarDeclaredName and `const a` a LexicallyDeclaredName.
negative:
  phase: parse
  type: SyntaxError
---*/

$DONOTEVALUATE();

(function () {
  const a = 1;
  function a() {}
})();
