// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-function-definitions-static-semantics-early-errors
description: function bodies reject lexical declarations that collide with var-scoped names, parameters or each other
info: |
  FunctionBody : FunctionStatementList

  It is a Syntax Error if the LexicallyDeclaredNames of FunctionStatementList
  contains any duplicate entries.

  It is a Syntax Error if any element of the LexicallyDeclaredNames of
  FunctionStatementList also occurs in the VarDeclaredNames of
  FunctionStatementList.

  FormalParameters : FormalParameterList

  It is a Syntax Error if any element of the BoundNames of FormalParameters
  also occurs in the LexicallyDeclaredNames of FunctionBody.

  8.2.5 Static Semantics: TopLevelVarDeclaredNames
  Top-level function declarations (also under labels) are VarDeclaredNames.
---*/

function bad(source) {
  assert.throws(SyntaxError, function () { Function(source); }, "Function: " + source);
  assert.throws(SyntaxError, function () { eval("(function () {" + source + "})"); }, "eval: " + source);
}

function good(source) {
  Function(source);
  eval("(function () {" + source + "})");
}

// Top-level function declarations are var-scoped
bad("const a = 1; function a() {}");
bad("function a() {} let a;");
bad("class a {} function a() {}");
bad("function* a() {} const a = 1;");
bad("async function a() {} let a;");
bad("let a; l: function a() {}");
bad("l: function a() {} const a = 1;");

// var declarations nested anywhere in the body
bad("let a; var a;");
bad("const a = 1; if (true) { var a; }");
bad("let a; for (var a;;) break;");
bad("let a; for (var a in {}) ;");
bad("class a {} try { } catch (e) { var a; }");
bad("let a; switch (0) { case 0: var a; }");

// Duplicate lexical declarations
bad("let a; let a;");
bad("const a = 1; class a {}");

// Parameters versus lexical declarations
assert.throws(SyntaxError, function () { Function("a", "class a {}"); }, "class vs parameter");
assert.throws(SyntaxError, function () { Function("a", "let a;"); }, "let vs parameter");
assert.throws(SyntaxError, function () { Function("{a}", "const a = 1;"); }, "const vs destructured parameter");

// Other function kinds share the same body rules
assert.throws(SyntaxError, function () { eval("() => { const a = 1; function a() {} }"); }, "arrow");
assert.throws(SyntaxError, function () { eval("async () => { let a; var a; }"); }, "async arrow");
assert.throws(SyntaxError, function () { eval("(async function () { const a = 1; function a() {} })"); }, "async function");
assert.throws(SyntaxError, function () { eval("(function* () { const a = 1; function a() {} })"); }, "generator");
assert.throws(SyntaxError, function () { eval("({ m() { let a; var a; } })"); }, "method");
assert.throws(SyntaxError, function () { eval("({ get g() { let a; var a; } })"); }, "getter");
assert.throws(SyntaxError, function () { eval("(class { m() { const a = 1; function a() {} } })"); }, "class method");

// Allowed forms
good("var a; function a() {}");
good("function a() {} function a() {}");
good("var a; var a;");
good("let a; { function a() {} }");
good("const a = 1; { let a; var b; }");
good("let a; function g() { var a; function a() {} }");
good("let a; l: { function a() {} }");
good("let a; (function a() {});");
Function("a", "function a() {}");
Function("a", "var a;");
Function("a", "{ let a; }");
