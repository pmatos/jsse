// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-with-statement-runtime-semantics-evaluation
description: >
  Inside an async generator, an `await using` declared directly inside a
  `with`-body binds into the block scope's own environment: the binding
  survives yields and awaits inside the scope, the with-object's own
  properties shadow outer bindings of the same name, and `with (null)` still
  throws a TypeError at `with` entry (issue #862).
info: |
  WithStatement : with ( Expression ) Statement

  [...]
  2. Let obj be ? ToObject(? GetValue(val)).
  [...]
  4. Let newEnv be NewObjectEnvironment(obj, true, oldEnv).
  5. Set the running execution context's LexicalEnvironment to newEnv.
  6. Let C be Completion(Evaluation of Statement).

  Each state produced while lowering the scope resumes at its own
  environment, so the chain must be `oldEnv -> withEnv -> blockEnv` on every
  resume, not a per-state throwaway block.
flags: [async, noStrict]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management, async-iteration]
---*/

asyncTest(async function () {
  var log = [];
  var it = (async function* () {
    with ({}) {
      await using a = {
        tag: 'resource',
        [Symbol.asyncDispose]() { log.push('dispose'); }
      };
      log.push('before-yield');
      yield 1;
      await 0;
      log.push(a.tag);
    }
    yield 2;
  })();
  assert.sameValue((await it.next()).value, 1);
  assert.sameValue((await it.next()).value, 2);
  assert.compareArray(
    log,
    ['before-yield', 'resource', 'dispose'],
    'the await-using binding survives the yield and await and still resolves to the resource'
  );
});

asyncTest(async function () {
  var outerName = 'outer';
  var log = [];
  var it = (async function* () {
    with ({ outerName: 'from-with' }) {
      await using a = { [Symbol.asyncDispose]() { log.push('dispose'); } };
      yield 0;
      log.push(outerName);
    }
  })();
  await it.next();
  await it.next();
  assert.compareArray(
    log,
    ['from-with', 'dispose'],
    'the with-object\'s own property shadows the outer binding for code inside the scope'
  );
  assert.sameValue(outerName, 'outer', 'the outer binding itself is untouched');
});

asyncTest(async function () {
  var log = [];
  var it = (async function* () {
    with ({ v: 'outer-with' }) {
      with ({ w: 'inner-with' }) {
        await using a = { [Symbol.asyncDispose]() { log.push('dispose'); } };
        yield v + '/' + w;
        return v + '|' + w;
      }
    }
  })();
  assert.sameValue((await it.next()).value, 'outer-with/inner-with', 'both with-objects are in scope');
  var done = await it.next();
  assert.sameValue(done.value, 'outer-with|inner-with', 'the return expression resolves through both with-objects');
  assert.sameValue(done.done, true);
  assert.compareArray(log, ['dispose'], 'the resource disposes on the way out');
});

asyncTest(async function () {
  var log = [];
  var it = (async function* () {
    with (null) {
      await using a = { [Symbol.asyncDispose]() { log.push('dispose'); } };
      yield 1;
    }
  })();
  await assert.throwsAsync(TypeError, function () { return it.next(); });
  assert.compareArray(log, [], 'nothing is declared or disposed when ToObject(null) throws');
});
