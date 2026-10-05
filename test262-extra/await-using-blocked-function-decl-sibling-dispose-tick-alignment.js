// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-disposeresources
description: >
  A block containing a nested `await using` block, with a sibling
  sloppy-mode `function` declaration in the same outer block, has its
  `Isolatable` reach downgraded to `Blocked` by the Annex B hoisting guard
  (sec-web-compat-functiondeclarationinstantiation): neither
  `transform_scope_block` nor the generic per-entry `ScopeAction::OpenBlock`
  path implements that hoisting, so the container stays on the tree-walker.
  When this `Blocked` reach is the function's *only* suspension point, the
  function must still take a real suspension-aware state so the nested
  block's disposal Await suspends the function instead of draining the job
  queue inline — and the sibling function must still be correctly hoisted to
  function scope, unaffected by this suspension fix.
info: |
  DisposeResources ( disposeCapability, completion )

  [...]
  3. For each element resource of disposeCapability.[[DisposableResourceStack]], in reverse list order, do
     [...]
     f. Else,
        i. Assert: hint is async-dispose.
        ii. Set needsAwait to true.
  4. If needsAwait is true and hasAwaited is false, then
     a. Perform ! Await(undefined).

  Annex B 3.3.1 Changes to FunctionDeclarationInstantiation

  [...]
  A block-scoped function declaration is additionally bound at
  function/script scope, so `typeof g` after the block must observe
  `"function"` regardless of how the block itself is executed.

  A witness chain of promise reactions is started before the function's
  promise gets its own reaction, so the position of "after" and "settled"
  pins the number of ticks the nested block's disposal consumed: it must
  suspend the function, not drain the queue inline.
flags: [async, noStrict]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management]
---*/

function observe(shape) {
  var log = [];
  var L = function (entry) { log.push(entry); };
  Promise.resolve()
    .then(function () { L('w1'); })
    .then(function () { L('w2'); })
    .then(function () { L('w3'); })
    .then(function () { L('w4'); });
  var promise = shape(L);
  promise.then(function () { L('settled'); }, function () { L('rejected'); });
  L('sync-end');
  var drain = Promise.resolve();
  for (var i = 0; i < 12; i++) {
    drain = drain.then(function () {});
  }
  return drain.then(function () { return log; });
}

asyncTest(async function () {
  var log = await observe(function (L) {
    return (async function () {
      {
        { await using a = { [Symbol.asyncDispose]() { L('disp-async'); } }; }
        function g() {}
      }
      L('after:' + typeof g);
    })();
  });
  assert.compareArray(
    log,
    ['disp-async', 'sync-end', 'w1', 'after:function', 'w2', 'settled', 'w3', 'w4'],
    'a sibling function declaration blocks the block reach but must still suspend the function, and g is still hoisted'
  );
});
