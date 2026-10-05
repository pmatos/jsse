// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-functiondeclarationinstantiation
description: >
  A sloppy-mode `function` declaration directly inside a `try`, `catch` or
  `finally` block is hoisted to the enclosing function's var scope
  (Annex B.3.3), including once the declaration sits beside an
  `await using` head that disposes at its Await.
info: |
  FunctionDeclarationInstantiation ( func, argumentsList )

  [...]
  <web-compat insertion point>: For each FunctionDeclaration f that is
  directly contained in the StatementList of a Block, CaseClause, or
  DefaultClause, in source text order, do
    1. Let F be StringValue of the BindingIdentifier of f.
    2. If replacing the FunctionDeclaration f with a VariableStatement that
       has F as a BindingIdentifier would not produce any Early Errors for
       func and F is not an element of parameterNames, then
       [...]

  The Block of a Try, Catch or Finally clause is a Block, so its directly
  contained function declarations are candidates (issue #848).
flags: [async, noStrict]
includes: [asyncHelpers.js]
features: [explicit-resource-management]
---*/

(function () {
  try { function g() { return 'g'; } } finally {}
  assert.sameValue(typeof g, 'function', 'try block, finally present');
})();

(function () {
  try { function g() { return 'g'; } } catch (e) {}
  assert.sameValue(typeof g, 'function', 'try block, catch present');
})();

(function () {
  try { throw 1; } catch (e) { function g() { return 'g'; } }
  assert.sameValue(typeof g, 'function', 'catch body');
})();

(function () {
  try {} finally { function g() { return 'g'; } }
  assert.sameValue(typeof g, 'function', 'finally body');
})();

(function () {
  assert.sameValue(g, undefined, 'binding is initialised to undefined at entry');
  try { function g() { return 'g'; } } finally {}
  assert.sameValue(g(), 'g', 'binding is assigned when the declaration is evaluated');
})();

(function () {
  try { throw [1]; } catch ([e]) { { function e() {} } }
  assert.sameValue(typeof e, 'undefined', 'destructured catch parameter blocks the hoist');
})();

(function () {
  try { throw 1; } catch (e) { { function e() {} } }
  assert.sameValue(typeof e, 'function', 'simple catch parameter does not block the hoist (B.3.5)');
})();

(function () {
  let g = 'lexical';
  try { function g() {} } finally {}
  assert.sameValue(g, 'lexical', 'an enclosing lexical declaration blocks the hoist');
})();

asyncTest(async function () {
  {
    function f() { return 'f'; }
    for (await using a = { [Symbol.asyncDispose]() {} }; false; ) {}
  }
  assert.sameValue(typeof f, 'function', 'block beside a for (await using) head');

  {
    function h() { return 'h'; }
    for await (await using a of [{ [Symbol.asyncDispose]() {} }]) {}
  }
  assert.sameValue(typeof h, 'function', 'block beside a for-await (await using) head');

  {
    await using a = { [Symbol.asyncDispose]() {} };
    function k() { return 'k'; }
  }
  assert.sameValue(typeof k, 'function', 'block co-declaring await using and a function');

  try {
    await 0;
    function m() { return 'm'; }
  } finally {}
  assert.sameValue(typeof m, 'function', 'try block in an async function with await');
});
