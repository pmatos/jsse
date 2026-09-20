/*---
description: >
  Only function declarations directly in the top-level StatementList are
  var-declared for the Script early error: duplicate function declarations,
  var/function pairs, function declarations nested in blocks, and names in
  inner function scopes do not collide with a script-level lexical binding.
esid: sec-static-semantics-toplevelvardeclarednames
info: |
  8.2.4 Static Semantics: TopLevelVarDeclaredNames
    At the top level of a function or script, inner function declarations are
    treated like var declarations.

  14.2.1 Block: Early Errors
    It is a Syntax Error if any element of the LexicallyDeclaredNames of
    StatementList also occurs in the VarDeclaredNames of StatementList.
    (Block-level function declarations are lexical, not var-declared.)
---*/

var indirectEval = eval;

[
  "function f() {} function f() {}",
  "'use strict'; function f() {} function f() {}",
  "var f; function f() {}",
  "let f; { function f() {} }",
  "let f; if (1) { function f() {} }",
  "let f; switch (1) { case 1: function f() {} }",
  "function f() { let f; }",
  "let f; function g() { var f; }",
].forEach(function(source) {
  indirectEval(source);
});
