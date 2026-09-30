// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  An `await using` block nested in a for-of body of an async generator
  disposes before the loop's iterator is closed, whether the loop is left by
  break, a throw, or `.return()` at a yield inside the block.
info: |
  ForIn/OfBodyEvaluation: if the body completes abruptly, IteratorClose runs
  after the body's own completion — the block's DisposeResources
  (proposal-explicit-resource-management, sec-disposeresources) is part of
  evaluating that body, so it finishes first.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management, async-iteration]
---*/

asyncTest(async function () {
  var log = [];
  var L = function (entry) { log.push(entry); };
  var disposable = function (name) {
    return {
      async [Symbol.asyncDispose]() { L('d-' + name); await null; L('d-' + name + '-end'); }
    };
  };
  var iterable = {
    [Symbol.iterator]() {
      var i = 0;
      return {
        next() { return { value: ++i, done: false }; },
        return() { L('iter-return'); return {}; }
      };
    }
  };

  var it = (async function* () {
    for (var x of iterable) {
      {
        await using a = disposable('a');
        if (x === 2) break;
        yield x;
      }
    }
    L('end');
  })();
  await it.next();
  await it.next();
  assert.compareArray(
    log,
    ['d-a', 'd-a-end', 'd-a', 'd-a-end', 'iter-return', 'end'],
    'break leaves the block, disposing it, before the iterator closes'
  );

  log = [];
  var it2 = (async function* () {
    for (var x of iterable) {
      {
        await using a = disposable('a');
        yield x;
        throw 'boom';
      }
    }
  })();
  await it2.next();
  try { await it2.next(); } catch (e) { L('caught-' + e); }
  assert.compareArray(
    log,
    ['d-a', 'd-a-end', 'iter-return', 'caught-boom'],
    'a throw disposes the block, then closes the iterator'
  );

  log = [];
  var it3 = (async function* () {
    for (var x of iterable) {
      {
        await using a = disposable('a');
        yield x;
      }
    }
  })();
  await it3.next();
  var r = await it3.return('R');
  assert.sameValue(r.value, 'R', '.return() value');
  assert.compareArray(
    log,
    ['d-a', 'd-a-end', 'iter-return'],
    '.return() at a yield disposes the block before closing the iterator'
  );
});
