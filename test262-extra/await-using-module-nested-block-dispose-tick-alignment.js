// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-disposeresources
description: >
  A top-level `await using` block nested in a try body or a loop body suspends
  the module at its disposal Await, so a promise reaction chain queued before
  the block advances one step per disposal instead of draining inline.
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

  Block : { StatementList }

  [...]
  6. Set blockValue to Completion(DisposeResources(blockEnv.[[DisposeCapability]], blockValue)).
flags: [module, async]
includes: [compareArray.js]
features: [dynamic-import, explicit-resource-management]
---*/

globalThis.moduleAwaitUsingLog = [];

import('./await-using-module-nested-block-dispose-tick-alignment_FIXTURE.mjs').then(
  function () {
    try {
      assert.compareArray(
        globalThis.moduleAwaitUsingLog,
        ['try-body', 'disposer', 'w1', 'caught-boom', 'loop1', 'w2', 'loop2', 'w3', 'end', 'w4'],
        'each disposal Await lets exactly one queued reaction run'
      );
      $DONE();
    } catch (assertionError) {
      $DONE(assertionError);
    }
  },
  $DONE
);
