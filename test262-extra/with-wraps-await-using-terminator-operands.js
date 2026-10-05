// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-with-statement-runtime-semantics-evaluation
description: >
  A `return` expression evaluated by the async-function state-machine
  driver itself (not through the ordinary per-statement AST path) still
  resolves an identifier through the with-environment chain built by
  `EnterScope`, for a `return` that follows an `await using` declared
  directly inside the same with-body (issue #858).
info: |
  WithStatement : with ( Expression ) Statement

  [...]
  4. Let newEnv be NewObjectEnvironment(obj, true, oldEnv).
  5. Set the running execution context's LexicalEnvironment to newEnv.
  6. Let C be Completion(Evaluation of Statement).
  [...]

  A `return` terminator's expression is evaluated directly against the
  state's environment by the driver, bypassing the AST-statement
  execution path the rest of a block's statements go through — this
  exercises a different code path than an ordinary statement inside the
  scope, so it needs its own coverage even though both read through the
  same environment chain.
flags: [async, noStrict]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management]
---*/

asyncTest(async function () {
  var log = [];
  var result = await (async function () {
    with ({ v: 'from-with' }) {
      await using a = { [Symbol.asyncDispose]() { log.push('dispose'); } };
      await 0;
      return v;
    }
  })();
  assert.sameValue(result, 'from-with', 'the return expression resolves v through the with-object');
  assert.compareArray(log, ['dispose'], 'the resource still disposes on the way out');
});
