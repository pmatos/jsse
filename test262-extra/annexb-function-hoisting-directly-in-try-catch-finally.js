// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-web-compat-functiondeclarationinstantiation
description: >
  A sloppy-mode `function` declaration directly in the statement list of a
  try block, catch block or finally block is hoisted to the enclosing
  function's var scope (Annex B.3.3), just like one in a bare block.
info: |
  B.3.2.1 Changes to FunctionDeclarationInstantiation

  For each FunctionDeclaration f that is directly contained in the
  StatementList of a Block, CaseClause, or DefaultClause:
    1. Let F be StringValue of the BindingIdentifier of f.
    2. If replacing the FunctionDeclaration f with a VariableStatement that
       has F as a BindingIdentifier would not produce any Early Errors for
       func and F is not an element of parameterNames, then
       [...] Perform ! varEnv.CreateMutableBinding(F, false) /
       InitializeBinding(F, undefined).

  The Block of a Try statement, a Catch clause and a Finally clause are all
  Block productions (issue #843).
flags: [noStrict]
---*/

function inTry() {
  try { function g() {} } finally {}
  return typeof g;
}
assert.sameValue(inTry(), "function", "try block");

function inCatch() {
  try { throw 0; } catch (e) { function g() {} }
  return typeof g;
}
assert.sameValue(inCatch(), "function", "catch block");

function inFinally() {
  try {} finally { function g() {} }
  return typeof g;
}
assert.sameValue(inFinally(), "function", "finally block");

function beforeEvaluation() {
  var before = typeof g;
  try { function g() {} } finally {}
  return before;
}
assert.sameValue(beforeEvaluation(), "undefined", "binding is undefined before block runs");

function blockedByLexical() {
  try { let g; { function g() {} } } finally {}
  return typeof g;
}
assert.sameValue(blockedByLexical(), "undefined", "enclosing let blocks hoisting");

try { function globalG() {} } finally {}
assert.sameValue(typeof globalG, "function", "global try block");
