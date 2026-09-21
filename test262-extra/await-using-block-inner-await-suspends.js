// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-block-runtime-semantics-evaluation
description: >
  An `await` inside an `await using` block, after the resource declaration,
  suspends the async function at that `await` and resumes past it instead of
  replaying the block (and its declaration) from the top.
info: |
  Block : { StatementList }

  1. Let oldEnv be the running execution context's LexicalEnvironment.
  2. Let blockEnv be NewDeclarativeEnvironment(oldEnv).
  [...]
  4. Set the running execution context's LexicalEnvironment to blockEnv.
  5. Let blockValue be Completion(Evaluation of StatementList).
  6. Set blockValue to Completion(DisposeResources(blockEnv.[[DisposeCapability]], blockValue)).
  7. Set the running execution context's LexicalEnvironment to oldEnv.
  8. Return ? blockValue.

  Each state produced while evaluating StatementList resumes at its own
  successor state, so an `await` that is not part of the block's own
  DisposeResources suspends exactly once and never re-declares the block's
  resources on resume.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management]
---*/

asyncTest(async function () {
  var log = [];
  var disposals = 0;
  await (async function () {
    try {
      {
        await using a = {
          [Symbol.asyncDispose]() {
            disposals++;
            log.push('dispose');
          }
        };
        log.push('before-await');
        await 0;
        log.push('after-await');
      }
    } catch (e) {
      log.push('caught-' + e);
    }
  })();
  assert.compareArray(
    log,
    ['before-await', 'after-await', 'dispose'],
    'the declaration runs exactly once and the inner await resumes past it'
  );
  assert.sameValue(disposals, 1, 'the resource is disposed exactly once');
});
