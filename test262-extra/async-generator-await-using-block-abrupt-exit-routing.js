// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-block-runtime-semantics-evaluation
description: >
  Leaving an `await using` block of an async generator by break, continue,
  return, or throw disposes the block's resources exactly once, whether or not
  the loop around it also yields.
info: |
  Block : { StatementList }

  DisposeResources (proposal-explicit-resource-management,
  sec-disposeresources) runs on the block's completion, whatever its kind:
  normal, break, continue, return, or throw.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management, async-iteration]
---*/

async function drain(it) {
  var values = [];
  while (true) {
    var r = await it.next();
    if (r.done) return values;
    values.push(r.value);
  }
}

asyncTest(async function () {
  var log = [];
  var L = function (entry) { log.push(entry); };
  var disposable = function (name) {
    return { async [Symbol.asyncDispose]() { L('d-' + name); } };
  };

  var values = await drain((async function* () {
    for (var i = 0; i < 3; i++) {
      {
        await using a = disposable('brk' + i);
        if (i === 1) break;
        yield i;
      }
    }
    L('after-loop');
  })());
  assert.compareArray(values, [0], 'break with yields');
  assert.compareArray(log, ['d-brk0', 'd-brk1', 'after-loop'], 'break disposes the block once');

  log = [];
  values = await drain((async function* () {
    for (var i = 0; i < 3; i++) {
      {
        await using a = disposable('cont' + i);
        if (i === 1) continue;
        yield i;
      }
    }
  })());
  assert.compareArray(values, [0, 2], 'continue with yields');
  assert.compareArray(log, ['d-cont0', 'd-cont1', 'd-cont2'], 'continue disposes each iteration once');

  log = [];
  values = await drain((async function* () {
    outer: for (var i = 0; i < 2; i++) {
      {
        await using a = disposable('o' + i);
        for (var j = 0; j < 2; j++) {
          {
            await using b = disposable('i' + i + j);
            if (j === 0) continue outer;
          }
        }
      }
    }
    L('done');
  })());
  assert.compareArray(
    log,
    ['d-i00', 'd-o0', 'd-i10', 'd-o1', 'done'],
    'a labelled continue disposes the inner then the outer scope'
  );

  log = [];
  var it = (async function* () {
    {
      await using a = disposable('ret');
      yield 1;
      return 'rv';
    }
  })();
  await it.next();
  var r = await it.next();
  assert.sameValue(r.value, 'rv', 'return value survives disposal');
  assert.sameValue(r.done, true, 'return completes the generator');
  assert.compareArray(log, ['d-ret'], 'return disposes the block');

  log = [];
  var it2 = (async function* () {
    try {
      {
        await using a = disposable('thr');
        yield 1;
        throw 'boom';
      }
    } catch (e) {
      L('caught-' + e);
    }
    L('post');
  })();
  await it2.next();
  await it2.next();
  assert.compareArray(log, ['d-thr', 'caught-boom', 'post'], 'a throw is caught after the block disposes');

  log = [];
  var it3 = (async function* () {
    {
      await using a = disposable('suspended');
      yield 1;
      L('not-reached');
    }
  })();
  await it3.next();
  await it3.return('early');
  assert.compareArray(log, ['d-suspended'], '.return() at a yield inside the block disposes it');

  log = [];
  var it4 = (async function* () {
    try {
      {
        await using a = disposable('thrown-in');
        yield 1;
        L('not-reached');
      }
    } catch (e) {
      L('caught-' + e);
    }
  })();
  await it4.next();
  await it4.throw('injected');
  assert.compareArray(log, ['d-thrown-in', 'caught-injected'], '.throw() at a yield disposes before the catch');

  log = [];
  var it5 = (async function* () {
    {
      await using a = { async [Symbol.asyncDispose]() { throw 'from-disposer'; } };
      throw 'original';
    }
  })();
  var caught;
  try { await it5.next(); } catch (e) { caught = e; }
  assert.sameValue(caught.constructor, SuppressedError, 'disposer error chains onto the in-flight throw');
  assert.sameValue(caught.error, 'from-disposer', 'error');
  assert.sameValue(caught.suppressed, 'original', 'suppressed');
});
