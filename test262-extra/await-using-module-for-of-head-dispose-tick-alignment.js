// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  A top-level `for (await using x of iterable)` head in a module suspends the
  module at its per-iteration disposal Await, so a promise reaction chain
  queued before the loop advances one step per disposal instead of draining
  inline.
info: |
  ForIn/OfBodyEvaluation ( lhs, stmt, iteratorRecord, iterationKind, lhsKind, labelSet [ , iteratorKind ] )

  [...]
  9.k. Else,
       ii. Set status to Completion(DisposeResources(iterationEnv.[[DisposeCapability]], result)).

  DisposeResources ( disposeCapability, completion )

  [...]
  3. For each element resource of disposeCapability.[[DisposableResourceStack]], in reverse list order, do
     [...]
     f. Else,
        i. Assert: hint is async-dispose.
        ii. Set needsAwait to true.
  4. If needsAwait is true and hasAwaited is false, then
     a. Perform ! Await(undefined).
flags: [module, async]
includes: [compareArray.js]
features: [dynamic-import, explicit-resource-management]
---*/

globalThis.moduleForOfHeadLog = [];

import('./await-using-module-for-of-head-dispose-tick-alignment_FIXTURE.mjs').then(
  function () {
    try {
      assert.compareArray(
        globalThis.moduleForOfHeadLog,
        ['body', 'disp', 'w1', 'body', 'disp2', 'w2', 'end', 'w3', 'w4'],
        'each iteration disposal Await lets exactly one queued reaction run'
      );
      $DONE();
    } catch (assertionError) {
      $DONE(assertionError);
    }
  },
  $DONE
);
