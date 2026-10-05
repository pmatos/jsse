// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-with-statement-runtime-semantics-evaluation
description: >
  An `await using` declared directly inside a `with`-body binds into the
  scope's own environment: the binding survives a suspension inside the
  scope (it is not re-declared or left in TDZ on resume), and the
  with-object's own properties still shadow an outer binding of the same
  name for code inside the scope — proving the environment chain is
  `oldEnv -> withEnv -> scope_env`, not some other order or a disconnected
  throwaway environment (issue #858).
info: |
  WithStatement : with ( Expression ) Statement

  [...]
  4. Let newEnv be NewObjectEnvironment(obj, true, oldEnv).
  5. Set the running execution context's LexicalEnvironment to newEnv.
  6. Let C be Completion(Evaluation of Statement).
  [...]

  NewObjectEnvironment ( O, IsWithEnvironment, E )

  1. Let env be a new Object Environment Record containing O as the binding object.
  2. Set env.[[IsWithEnvironment]] to IsWithEnvironment.
  3. Set env.[[OuterEnv]] to E.
  4. Return env.

  Each state produced while lowering the scope resumes at its own
  successor state against the same scope environment, so a binding
  declared before a suspension point must still resolve after it, and an
  identifier reference inside the scope must still resolve through the
  with-object before falling through to an outer binding of the same name.
flags: [async, noStrict]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management]
---*/

asyncTest(async function () {
  var log = [];
  await (async function () {
    with ({}) {
      await using a = {
        tag: 'resource',
        [Symbol.asyncDispose]() { log.push('dispose'); }
      };
      log.push('before-await');
      await 0;
      log.push(a.tag);
    }
  })();
  assert.compareArray(
    log,
    ['before-await', 'resource', 'dispose'],
    'the await-using binding survives the suspension and still resolves to the resource afterward'
  );
});

asyncTest(async function () {
  var outerName = 'outer';
  var log = [];
  await (async function () {
    with ({ outerName: 'from-with' }) {
      await using a = { [Symbol.asyncDispose]() { log.push('dispose'); } };
      await 0;
      log.push(outerName);
    }
  })();
  assert.compareArray(
    log,
    ['from-with', 'dispose'],
    'the with-object\'s own property shadows an outer binding of the same name for code inside the scope'
  );
  assert.sameValue(outerName, 'outer', 'the outer binding itself is untouched');
});
