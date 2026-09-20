/*---
description: >
  A named async function / async generator expression whose own body carries a
  "use strict" directive must reject a strict-mode reserved word as its name,
  matching plain function expressions and function declarations.
info: |
  Identifiers -- Static Semantics: Early Errors
    Identifier : IdentifierName but not ReservedWord
      It is a Syntax Error if the goal symbol of the syntactic grammar is
      Module or if IsStrict(this phrase) is true and the StringValue of
      IdentifierName is one of "implements", "interface", "let", "package",
      "private", "protected", "public", "static", or "yield".

  Strict Mode Code
    Function code is strict mode code if the associated FunctionDeclaration,
    FunctionExpression, AsyncFunctionExpression, AsyncGeneratorExpression, ...
    is contained in strict mode code or if the code that produces the value of
    the function's [[ECMAScriptCode]] internal slot begins with a Directive
    Prologue that contains a Use Strict Directive.

  The BindingIdentifier of the expression is part of the function and is
  therefore strict when the body has a Use Strict Directive, even though the
  name is lexically before the directive.
esid: sec-identifiers-static-semantics-early-errors
features: [async-functions, async-iteration]
---*/

var reserved = [
  "implements", "interface", "let", "package", "private",
  "protected", "public", "static", "yield"
];

function indirectEval(src) {
  return (0, eval)(src);
}

function expectSyntaxError(src) {
  assert.throws(SyntaxError, function () {
    indirectEval(src);
  }, src);
}

function expectFunction(src) {
  assert.sameValue(typeof indirectEval(src), "function", src);
}

reserved.forEach(function (name) {
  expectSyntaxError("(async function " + name + "(){'use strict';})");
  expectSyntaxError("(async function* " + name + "(){'use strict';})");
});

// Sibling forms already reject the name; documents the invariant.
reserved.forEach(function (name) {
  expectSyntaxError("(function " + name + "(){'use strict';})");
  expectSyntaxError("(function* " + name + "(){'use strict';})");
  expectSyntaxError("(async function " + name + "(){'use strict';})");
  expectSyntaxError("async function " + name + "(){'use strict';}");
  expectSyntaxError("async function* " + name + "(){'use strict';}");
});

// Strictness inherited from the enclosing code takes the earlier check path.
reserved.forEach(function (name) {
  expectSyntaxError("'use strict'; (async function " + name + "(){})");
  expectSyntaxError("'use strict'; (async function* " + name + "(){})");
});

// eval and arguments are rejected by the same BindingIdentifier rule.
["eval", "arguments"].forEach(function (name) {
  expectSyntaxError("(async function " + name + "(){'use strict';})");
  expectSyntaxError("(async function* " + name + "(){'use strict';})");
});

// Without a strict context the names are ordinary identifiers.
["implements", "interface", "package", "private", "protected", "public"]
  .forEach(function (name) {
    expectFunction("(async function " + name + "(){})");
    expectFunction("(async function* " + name + "(){})");
  });
["let", "static", "yield"].forEach(function (name) {
  expectFunction("(async function " + name + "(){})");
});

// A non-reserved name with a directive is fine.
expectFunction("(async function foo(){'use strict';})");
expectFunction("(async function* foo(){'use strict';})");
