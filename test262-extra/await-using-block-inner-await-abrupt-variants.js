// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-block-runtime-semantics-evaluation
description: >
  An `await using` block whose interior contains an `await` unrelated to its
  own disposal disposes its resource exactly once, whether the statements
  after that `await` throw, declare a second resource, or run to completion.
info: |
  Block : { StatementList }

  [...]
  5. Let blockValue be Completion(Evaluation of StatementList).
  6. Set blockValue to Completion(DisposeResources(blockEnv.[[DisposeCapability]], blockValue)).
  [...]

  DisposeResources ( disposeCapability, completion )

  1. If disposeCapability.[[DisposableResourceStack]] is empty, then
     a. Return completion.
  [...]
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management]
---*/

asyncTest(async function () {
  // A throw after the inner await still disposes exactly once, and the
  // exception propagates past the block's own catch-free scope.
  var log = [];
  var disposals = 0;
  var caught;
  try {
    await (async function () {
      {
        await using a = {
          [Symbol.asyncDispose]() {
            disposals++;
            log.push('dispose-a');
          }
        };
        await 0;
        throw new Test262Error('from-block');
      }
    })();
  } catch (e) {
    caught = e;
  }
  assert.sameValue(caught && caught.message, 'from-block', 'the throw after await propagates');
  assert.sameValue(disposals, 1, 'the resource disposes exactly once despite the throw');
  assert.compareArray(log, ['dispose-a'], 'disposal happened before the throw left the function');

  // A second `await` after the block's own resource declaration, with a
  // second `await using` declared after the first await, disposes both
  // resources exactly once each, in reverse declaration order.
  log = [];
  await (async function () {
    {
      await using a = { [Symbol.asyncDispose]() { log.push('dispose-a'); } };
      await 0;
      await using b = { [Symbol.asyncDispose]() { log.push('dispose-b'); } };
      await 0;
      log.push('body');
    }
  })();
  assert.compareArray(
    log,
    ['body', 'dispose-b', 'dispose-a'],
    'both resources dispose exactly once, in reverse declaration order'
  );
});
