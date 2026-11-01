// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-class-definitions-static-semantics-early-errors
description: eval and arguments cannot be the BindingIdentifier of a class, because class code is strict
info: |
  ClassDeclaration : class BindingIdentifier ClassTail
  ClassExpression : class BindingIdentifier_opt ClassTail

  All parts of a ClassDefinition are strict mode code (11.2.2), and in strict
  mode code it is a Syntax Error if the StringValue of a BindingIdentifier is
  "eval" or "arguments" (13.1.1).
---*/

["class eval {}", "class arguments {}", "(class eval {})", "(class arguments {})", "x = class eval {}"].forEach(
  function(source) {
    assert.throws(SyntaxError, function() {
      Function(source);
    }, source);
  }
);

["class evalx {}", "(class arguments_ {})", "class C { eval() {} arguments() {} }"].forEach(function(source) {
  Function(source);
});
