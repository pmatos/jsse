// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-web-compat-functiondeclarationinstantiation
description: >
  A sloppy-mode function declaration next to an `await using` block, in a
  plain block, a `for (let ...)` body, a `for-in` body, or a `for-of` body
  headed by `let`/`const`/`using`, is still hoisted to the enclosing function
  scope (Annex B.3.3) once the container is lowered to isolate the block's
  disposal.
info: |
  B.3.3.1 Changes to FunctionDeclarationInstantiation

  [...]
  2. If instantiatedVarNames does not contain F, then
     a. Perform ! varEnvRec.CreateMutableBinding(F, false).
     b. Perform varEnvRec.InitializeBinding(F, undefined).
     c. Append F to instantiatedVarNames.
  [...]

  The scan that decides whether a container's lexical scope can be flattened
  into the generator-transform's state graph (`scan_await_using` /
  `scan_flattened_list` in `generator_analysis.rs`) must keep a container
  blocked when it reaches a `function` declaration: neither
  `transform_scope_block`'s `EnterScope`/`ExitScope` pair nor the generic
  per-entry `ScopeAction::OpenBlock` path runs this hoisting step, so such a
  container must stay in the tree-walker instead of being lowered.
flags: [async, noStrict]
includes: [asyncHelpers.js]
features: [explicit-resource-management]
---*/

asyncTest(async function () {
  {
    function g() {}
    {
      await using a = null;
    }
  }
  assert.sameValue(typeof g, 'function', 'function sibling in the outer block of a nested await-using block');

  for (let i = 0; i < 1; i++) {
    await using a = null;
    function h() {}
  }
  assert.sameValue(typeof h, 'function', 'function declared in a for (let ...) body with await using');

  for (var k in { a: 1 }) {
    await using a = null;
    function j() {}
  }
  assert.sameValue(typeof j, 'function', 'function declared in a for-in body with await using');

  for (let x of [1]) {
    await using a = null;
    function s() {}
  }
  assert.sameValue(typeof s, 'function', 'function declared in a for-of body headed by let with await using');

  for (const x of [1]) {
    await using a = null;
    function t() {}
  }
  assert.sameValue(typeof t, 'function', 'function declared in a for-of body headed by const with await using');

  for (using x of [{ [Symbol.dispose]() {} }]) {
    await using a = null;
    function u() {}
  }
  assert.sameValue(typeof u, 'function', 'function declared in a for-of body headed by using with await using');

  {
    let m = 1;
    {
      await using a = null;
      function n() {}
    }
  }
  assert.sameValue(
    typeof n,
    'function',
    'function declared beside an await-using declaration inside a block that itself sits beside a lexical sibling'
  );

  try {
    let p = 1;
    {
      await using a = null;
      function q() {}
    }
  } finally {
  }
  assert.sameValue(
    typeof q,
    'function',
    'function declared beside an await-using declaration inside a try block that itself sits beside a lexical sibling'
  );

  {
    {
      function r() {}
    }
    {
      await using a = null;
    }
  }
  assert.sameValue(
    typeof r,
    'function',
    'function declared in a block that is itself a sibling of the await-using block'
  );
});
