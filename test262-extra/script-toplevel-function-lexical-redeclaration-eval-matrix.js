/*---
description: >
  Indirect and direct eval parse their source as a Script, so a function
  declaration (plain, generator, async, async generator, labelled) and a
  lexical declaration of the same name at the top level are an early error
  in either order.
esid: sec-scripts-static-semantics-early-errors
info: |
  16.1.1 Static Semantics: Early Errors (Script : ScriptBody)
    It is a Syntax Error if any element of the LexicallyDeclaredNames of
    ScriptBody also occurs in the VarDeclaredNames of ScriptBody.

  8.2.4 TopLevelVarDeclaredNames
    StatementListItem : Declaration
      If Declaration is Declaration : HoistableDeclaration, return the
      BoundNames of HoistableDeclaration.
    LabelledItem : FunctionDeclaration
      Return the BoundNames of FunctionDeclaration.
features: [generators, async-functions, async-iteration]
---*/

var functions = [
  "function f() {}",
  "function* f() {}",
  "async function f() {}",
  "async function* f() {}",
  "l: function f() {}",
  "a: b: function f() {}",
];
var lexicals = ["let f;", "const f = 1;", "class f {}"];

var indirectEval = eval;

functions.forEach(function(fn) {
  lexicals.forEach(function(lex) {
    [fn + " " + lex, lex + " " + fn].forEach(function(source) {
      assert.throws(SyntaxError, function() {
        indirectEval(source);
      }, "indirect eval: " + source);

      assert.throws(SyntaxError, function() {
        eval(source);
      }, "direct eval: " + source);
    });
  });
});
