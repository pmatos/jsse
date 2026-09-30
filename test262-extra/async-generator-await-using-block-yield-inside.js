// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-block-runtime-semantics-evaluation
description: >
  A block that declares `await using` and contains a `yield` in an async
  generator is entered once: its resource is disposed once, when the block is
  left after the generator resumes, and its statements are not replayed.
info: |
  Block : { StatementList }

  1. Let oldEnv be the running execution context's LexicalEnvironment.
  2. Let blockEnv be NewDeclarativeEnvironment(oldEnv).
  [...]
  5. Let blockValue be Completion(Evaluation of StatementList).
  [...] DisposeResources (proposal-explicit-resource-management,
  sec-disposeresources) runs on the block's completion, whatever its kind.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management, async-iteration]
---*/

asyncTest(async function () {
  var log = [];
  var L = function (entry) { log.push(entry); };
  var it = (async function* () {
    {
      await using a = { async [Symbol.asyncDispose]() { L('d'); } };
      L('body');
      yield 1;
      L('after-yield');
    }
    L('post');
  })();
  var first = await it.next();
  assert.sameValue(first.value, 1, 'first next yields from inside the block');
  assert.compareArray(log, ['body'], 'the resource is not disposed while the block is suspended');
  var second = await it.next();
  assert.sameValue(second.done, true, 'second next completes the generator');
  assert.compareArray(
    log,
    ['body', 'after-yield', 'd', 'post'],
    'the block ran once and disposed before the statement after it'
  );

  log = [];
  var it2 = (async function* () {
    {
      await using a = { async [Symbol.asyncDispose]() { L('d-outer'); } };
      {
        await using b = { async [Symbol.asyncDispose]() { L('d-inner'); } };
        yield 'in';
        L('inner-end');
      }
      L('outer-end');
    }
    L('post');
  })();
  await it2.next();
  await it2.next();
  assert.compareArray(
    log,
    ['inner-end', 'd-inner', 'outer-end', 'd-outer', 'post'],
    'nested blocks dispose innermost first, each at its own exit'
  );
});
