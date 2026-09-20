/*---
description: A top-level function declaration followed by a const of the same name is an early error.
esid: sec-scripts-static-semantics-early-errors
info: |
  16.1.1 Static Semantics: Early Errors (Script : ScriptBody)
    It is a Syntax Error if any element of the LexicallyDeclaredNames of
    ScriptBody also occurs in the VarDeclaredNames of ScriptBody.

  At the top level of a script, ScriptBody's VarDeclaredNames are the
  TopLevelVarDeclaredNames of its StatementList, which include the BoundNames
  of function declarations.
negative:
  phase: parse
  type: SyntaxError
---*/

$DONOTEVALUATE();

function f() {}
const f = 1;
