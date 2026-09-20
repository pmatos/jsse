// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-disposeresources
description: >
  A break, continue, return or throw completion leaving an `await using` block
  disposes the block's resources first and then continues as that completion,
  including when the block sits inside a loop of an async function that awaits.
info: |
  Block : { StatementList }

  [...]
  5. Let blockValue be Completion(Evaluation of StatementList).
  6. Set blockValue to Completion(DisposeResources(blockEnv.[[DisposeCapability]], blockValue)).
  7. Set the running execution context's LexicalEnvironment to oldEnv.
  8. Return ? blockValue.

  DisposeResources returns the completion it was given once every disposer has
  run, so an abrupt completion resumes its original target afterwards.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management]
---*/

function resource(log, name) {
  return {
    async [Symbol.asyncDispose]() {
      log.push('dispose-' + name);
    }
  };
}

asyncTest(async function () {
  var log = [];
  var i = 0;
  while (true) {
    await 0;
    {
      await using a = resource(log, 'a');
      if (++i > 2) break;
      log.push('body' + i);
    }
  }
  log.push('after');
  assert.compareArray(
    log,
    ['body1', 'dispose-a', 'body2', 'dispose-a', 'dispose-a', 'after'],
    'break leaves a while loop after disposing'
  );

  log = [];
  for (var n = 0; n < 3; n++) {
    await 0;
    {
      await using a = resource(log, 'a');
      if (n === 1) continue;
      log.push('body' + n);
    }
    log.push('tail' + n);
  }
  assert.compareArray(
    log,
    ['body0', 'dispose-a', 'tail0', 'dispose-a', 'body2', 'dispose-a', 'tail2'],
    'continue skips the loop tail after disposing and still runs the update'
  );

  log = [];
  outer: while (true) {
    while (true) {
      await 0;
      {
        await using a = resource(log, 'a');
        break outer;
      }
    }
  }
  log.push('after');
  assert.compareArray(log, ['dispose-a', 'after'], 'labeled break leaves the outer loop');

  log = [];
  outerContinue: for (var p = 0; p < 2; p++) {
    for (var q = 0; q < 2; q++) {
      await 0;
      {
        await using a = resource(log, 'a' + p + q);
        continue outerContinue;
      }
      log.push('unreached-inner');
    }
    log.push('unreached-outer');
  }
  assert.compareArray(
    log,
    ['dispose-a00', 'dispose-a10'],
    'labeled continue resumes the outer loop after disposing'
  );

  log = [];
  var m = 0;
  do {
    await 0;
    {
      await using a = resource(log, 'a');
      await using b = resource(log, 'b');
      if (++m === 2) break;
      continue;
    }
  } while (true);
  assert.compareArray(
    log,
    ['dispose-b', 'dispose-a', 'dispose-b', 'dispose-a'],
    'resources are disposed in reverse order before break and continue'
  );

  log = [];
  var value = await (async function () {
    while (true) {
      await 0;
      {
        await using a = resource(log, 'a');
        return 'returned';
      }
    }
  })();
  assert.sameValue(value, 'returned', 'return value survives disposal');
  assert.compareArray(log, ['dispose-a'], 'return disposes first');

  log = [];
  var caught;
  try {
    await (async function () {
      while (true) {
        await 0;
        {
          await using a = resource(log, 'a');
          throw new Test262Error('from-block');
        }
      }
    })();
  } catch (e) {
    caught = e;
  }
  assert.sameValue(caught && caught.message, 'from-block', 'throw survives disposal');
  assert.compareArray(log, ['dispose-a'], 'throw disposes first');

  log = [];
  var r = 0;
  while (true) {
    await 0;
    try {
      {
        await using a = resource(log, 'a');
        if (++r > 1) break;
        log.push('body' + r);
      }
    } finally {
      log.push('finally' + r);
    }
  }
  log.push('after');
  assert.compareArray(
    log,
    ['body1', 'dispose-a', 'finally1', 'dispose-a', 'finally2', 'after'],
    'break out of a block inside try runs disposal then finally'
  );

  log = [];
  outerFinally: for (var s = 0; s < 2; s++) {
    for (var t = 0; t < 2; t++) {
      await 0;
      try {
        {
          await using a = resource(log, 'a' + s + t);
          continue outerFinally;
        }
      } finally {
        log.push('finally' + s + t);
      }
    }
  }
  log.push('after');
  assert.compareArray(
    log,
    ['dispose-a00', 'finally00', 'dispose-a10', 'finally10', 'after'],
    'labeled continue out of a block inside try runs disposal then finally'
  );

  log = [];
  var iterable = {
    [Symbol.iterator]() {
      return {
        i: 0,
        next() {
          return { done: this.i > 3, value: this.i++ };
        },
        return() {
          log.push('iterator-closed');
          return {};
        }
      };
    }
  };
  for (var x of iterable) {
    try {
      {
        await using a = resource(log, 'a' + x);
        if (x === 1) break;
      }
    } catch (e) {
    }
  }
  log.push('after');
  assert.compareArray(
    log,
    ['dispose-a0', 'dispose-a1', 'iterator-closed', 'after'],
    'break out of a block inside a for-of body disposes before the iterator closes'
  );

  log = [];
  for (var u = 0; u < 2; u++) {
    try {
      {
        await using a = resource(log, 'a' + u);
        throw u;
      }
    } catch (e) {
      log.push('caught' + e);
      continue;
    } finally {
      log.push('finally' + u);
    }
  }
  log.push('after');
  assert.compareArray(
    log,
    ['dispose-a0', 'caught0', 'finally0', 'dispose-a1', 'caught1', 'finally1', 'after'],
    'throw out of a block inside try is caught after disposal, then continue runs finally'
  );

  log = [];
  var w = 0;
  outerSwitch: while (w < 3) {
    w++;
    switch (w) {
      case 1: {
        await using a = resource(log, 'a' + w);
        continue outerSwitch;
      }
      case 2: {
        await using a = resource(log, 'a' + w);
        break;
      }
    }
    log.push('tail' + w);
  }
  log.push('after');
  assert.compareArray(
    log,
    ['dispose-a1', 'dispose-a2', 'tail2', 'tail3', 'after'],
    'continue and break out of switch-case blocks'
  );
});
