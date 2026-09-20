/*---
description: >
  A named async function / async generator expression whose own body carries a
  "use strict" directive must reject a strict-mode reserved word as its name,
  matching plain function expressions and function declarations.
info: |
  Identifiers -- Static Semantics: Early Errors
    Identifier : IdentifierName but not ReservedWord
      It is a Syntax Error if IsStrict(this phrase) is true and the
      StringValue of IdentifierName is one of "implements", "interface", "let",
      "package", "private", "protected", "public", "static", or "yield".

  Strict Mode Code
    Function code is strict mode code if the associated FunctionDeclaration,
    FunctionExpression, AsyncFunctionExpression, AsyncGeneratorExpression, ...
    is contained in strict mode code or if the code that produces the value of
    the function's [[ECMAScriptCode]] internal slot begins with a Directive
    Prologue that contains a Use Strict Directive.

  The BindingIdentifier of the expression is part of the function and is
  therefore strict when the body has a Use Strict Directive, even though the
  name is lexically before the directive.

  Sources are compiled with indirect eval, which ignores the strictness of
  this file, so a strict variant would only repeat the same assertions.
esid: sec-identifiers-static-semantics-early-errors
features: [async-functions, async-iteration]
flags: [noStrict]
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
  expectSyntaxError("(async function " + name + "(){\"use strict\";})");
  expectSyntaxError("(async function " + name + "(){'a'; 'use strict';})");
  expectSyntaxError("(async function " + name + "(){\n'use strict'\n})");
});

reserved.forEach(function (name) {
  expectSyntaxError("(function " + name + "(){'use strict';})");
  expectSyntaxError("(function* " + name + "(){'use strict';})");
  expectSyntaxError("async function " + name + "(){'use strict';}");
  expectSyntaxError("async function* " + name + "(){'use strict';}");
});

reserved.forEach(function (name) {
  expectSyntaxError("'use strict'; (async function " + name + "(){})");
  expectSyntaxError("'use strict'; (async function* " + name + "(){})");
});

expectSyntaxError("(async function l\\u0065t(){'use strict';})");
expectSyntaxError("(async function st\\u0061tic(){'use strict';})");
expectSyntaxError("(async function yi\\u0065ld(){'use strict';})");

["eval", "arguments"].forEach(function (name) {
  expectSyntaxError("(async function " + name + "(){'use strict';})");
  expectSyntaxError("(async function* " + name + "(){'use strict';})");
});

// The name is always a syntax error here, with or without a directive.
expectSyntaxError("(async function* yield(){})");
expectSyntaxError("(async function await(){})");
expectSyntaxError("(async function* await(){})");

reserved.forEach(function (name) {
  expectFunction("(async function " + name + "(){})");
  if (name !== "yield") {
    expectFunction("(async function* " + name + "(){})");
  }
});

expectFunction("(async function foo(){'use strict';})");
expectFunction("(async function* foo(){'use strict';})");

// A string that is not a Use Strict Directive leaves the name sloppy.
expectFunction("(async function let(){'use\\x20strict';})");
expectFunction("(async function let(){0; 'use strict';})");
expectFunction("(async function let(){'use strict'.length;})");
expectFunction("(async function let(){function f(){'use strict';}})");
expectFunction("(async function let(){(async function(){'use strict';});})");

// Method names are not BindingIdentifiers.
expectFunction("({async let(){'use strict';}}).let");
expectFunction("({async* static(){'use strict';}}).static");
expectFunction("(class { static async implements(){'use strict';} }).implements");
