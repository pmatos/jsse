// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-for-statement-runtime-semantics-forloopevaluation
description: >
  A `for (using x = init; test; update)` head disposes its loop environment
  once, when the loop exits, before the code following the loop runs. This
  holds when the enclosing function needs the state-machine lowering because
  of an unrelated `await` (plain async function) or `yield` (sync generator).
info: |
  ForStatement : for ( LexicalDeclaration Expression_opt ; Expression_opt ) Statement

  [...]
  7. Let bodyResult be Completion(ForBodyEvaluation(test, increment, Statement, perIterationLets, labelSet)).
  8. Set bodyResult to DisposeResources(loopEnv.[[DisposeCapability]], bodyResult).
  9. Set the running execution context's LexicalEnvironment to oldEnv.
  10. Return ? bodyResult.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management]
---*/

function drain(it) {
  var values = [];
  var step;
  while (!(step = it.next()).done) {
    values.push(step.value);
  }
  return values;
}

asyncTest(async function () {
  var log = [];
  var L = function (entry) { log.push(entry); };
  var disposer = function (name) {
    return { [Symbol.dispose]() { L(name); } };
  };

  log = [];
  await (async function () {
    for (using r = disposer('disp'); false; ) {}
    await 0;
    L('after');
  })();
  assert.compareArray(log, ['disp', 'after'], 'async function: loop that never enters its body');

  log = [];
  await (async function () {
    var i = 0;
    for (using r = disposer('disp'); i < 3; i++) {
      L('body' + i);
      await 0;
    }
    L('after');
  })();
  assert.compareArray(
    log,
    ['body0', 'body1', 'body2', 'disp', 'after'],
    'async function: disposed once, at loop exit, not once per iteration'
  );

  log = [];
  await (async function () {
    for (using r = disposer('disp'); true; ) {
      await 0;
      break;
    }
    L('after');
  })();
  assert.compareArray(log, ['disp', 'after'], 'async function: break exit');

  log = [];
  await (async function () {
    var n = 0;
    outer: for (var k = 0; k < 2; k++) {
      for (using r = disposer('disp' + k); n < 5; n++) {
        L('body' + k);
        await 0;
        continue outer;
      }
    }
    L('after');
  })();
  assert.compareArray(
    log,
    ['body0', 'disp0', 'body1', 'disp1', 'after'],
    'async function: continue to an outer label disposes the inner loop environment'
  );

  log = [];
  await (async function () {
    try {
      for (using r = disposer('disp'); true; ) {
        await 0;
        throw new Error('boom');
      }
    } catch (e) {
      L('caught-' + e.message);
    }
    L('after');
  })();
  assert.compareArray(
    log,
    ['disp', 'caught-boom', 'after'],
    'async function: a throw from the body disposes before the catch runs'
  );

  log = [];
  await (async function () {
    for (using a = disposer('dispA'), b = disposer('dispB'); false; ) {}
    await 0;
    L('after');
  })();
  assert.compareArray(
    log,
    ['dispB', 'dispA', 'after'],
    'async function: declarators dispose in reverse order'
  );

  log = [];
  function* normalExit() {
    for (using r = disposer('disp'); false; ) {}
    yield 1;
    L('after');
  }
  assert.compareArray(drain(normalExit()), [1], 'generator: yielded values (normal exit)');
  assert.compareArray(log, ['disp', 'after'], 'generator: loop that never enters its body');

  log = [];
  function* breakExit() {
    for (using r = disposer('disp'); true; ) {
      yield 1;
      break;
    }
    L('after');
  }
  var it = breakExit();
  it.next();
  it.next();
  assert.compareArray(log, ['disp', 'after'], 'generator: break exit');

  log = [];
  function* iterations() {
    var i = 0;
    for (using r = disposer('disp'); i < 3; i++) {
      L('body' + i);
      yield i;
    }
    L('after');
  }
  assert.compareArray(drain(iterations()), [0, 1, 2], 'generator: yielded values (iterations)');
  assert.compareArray(
    log,
    ['body0', 'body1', 'body2', 'disp', 'after'],
    'generator: disposed once, at loop exit'
  );

  log = [];
  function* continueOuter() {
    outer: for (var k = 0; k < 2; k++) {
      for (using r = disposer('disp' + k); true; ) {
        yield k;
        continue outer;
      }
    }
    L('after');
  }
  drain(continueOuter());
  assert.compareArray(
    log,
    ['disp0', 'disp1', 'after'],
    'generator: continue to an outer label disposes the inner loop environment'
  );

  log = [];
  function* returned() {
    for (using r = disposer('disp'); true; ) {
      yield 1;
    }
  }
  it = returned();
  it.next();
  assert.sameValue(it.return(7).value, 7, 'generator: return() result');
  assert.compareArray(log, ['disp'], 'generator: return() at a yield disposes the loop environment');

  log = [];
  function* thrown() {
    for (using r = disposer('disp'); true; ) {
      yield 1;
      throw new Error('boom');
    }
  }
  it = thrown();
  it.next();
  assert.throws(Error, function () { it.next(); }, 'generator: body throw propagates');
  assert.compareArray(log, ['disp'], 'generator: a throw from the body disposes the loop environment');

  log = [];
  function* blockScope() {
    {
      using r = disposer('disp');
      yield 1;
    }
    L('after');
    yield 2;
  }
  it = blockScope();
  it.next();
  it.next();
  assert.compareArray(log, ['disp', 'after'], 'generator: a plain block declaring `using` disposes at its own exit');
});
