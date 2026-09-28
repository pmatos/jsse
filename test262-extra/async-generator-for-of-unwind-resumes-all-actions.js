// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgeneratorstart
description: >
  Async-generator for-of unwinds suspend at each Await of DisposeResources for
  source returns, injected returns, and loop control, without replaying the
  state that initiated the unwind.
info: |
  DisposeResources (proposal-explicit-resource-management,
  sec-disposeresources) Awaits an async disposer's result. Abrupt completion of
  a for-of body disposes the body's resources before IteratorClose, while an
  async-generator `return Expression` first Awaits the expression value.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management, async-iteration]
---*/

function resource(log, name) {
  return {
    [Symbol.asyncDispose]() {
      log.push('dispose-' + name + '-start');
      return Promise.resolve().then(function () {
        log.push('dispose-' + name + '-end');
      });
    }
  };
}

function iterable(log, name, values) {
  return {
    [Symbol.iterator]() {
      var index = 0;
      return {
        next() {
          return index < values.length
            ? { value: values[index++], done: false }
            : { value: undefined, done: true };
        },
        return() {
          log.push('close-' + name);
          return {};
        }
      };
    }
  };
}

asyncTest(async function () {
  var log = [];
  var it = (async function* () {
    for (var value of iterable(log, 'expr', [1])) {
      {
        await using item = resource(log, 'expr');
        yield value;
        log.push('return-expr');
        return Promise.resolve(5);
      }
    }
  })();
  await it.next();
  var pending = it.next();
  log.push('after-next');
  assert.compareArray(
    log,
    ['return-expr', 'after-next'],
    'return Expression awaits its operand before starting the for-of unwind'
  );
  var result = await pending;
  assert.sameValue(result.value, 5, 'return Expression result');
  assert.sameValue(result.done, true, 'return Expression completes the generator');
  assert.compareArray(
    log,
    ['return-expr', 'after-next', 'dispose-expr-start', 'dispose-expr-end', 'close-expr'],
    'return Expression resumes the unwind once without replaying its state'
  );

  log = [];
  it = (async function* () {
    for (var value of iterable(log, 'bare', [1])) {
      {
        await using item = resource(log, 'bare');
        yield value;
        log.push('bare-return');
        return;
      }
    }
  })();
  await it.next();
  pending = it.next();
  log.push('after-next');
  assert.compareArray(
    log,
    ['bare-return', 'dispose-bare-start', 'after-next'],
    'bare return parks at the disposer Await instead of draining it inline'
  );
  result = await pending;
  assert.sameValue(result.value, undefined, 'bare return value');
  assert.sameValue(result.done, true, 'bare return completes the generator');
  assert.compareArray(
    log,
    ['bare-return', 'dispose-bare-start', 'after-next', 'dispose-bare-end', 'close-bare'],
    'bare return resumes the unwind once'
  );

  log = [];
  it = (async function* () {
    for (var value of iterable(log, 'injected', [1])) {
      {
        await using item = resource(log, 'injected');
        yield value;
      }
    }
  })();
  await it.next();
  var returned = it.return('R');
  var queued = it.next();
  result = await returned;
  assert.sameValue(result.value, 'R', 'injected return value');
  assert.sameValue(result.done, true, 'injected return completes the generator');
  assert.sameValue((await queued).done, true, 'a queued request stays behind the unwind');
  assert.compareArray(
    log,
    ['dispose-injected-start', 'dispose-injected-end', 'close-injected'],
    'injected return disposes and closes exactly once'
  );

  log = [];
  it = (async function* () {
    outer: for (var outerValue of iterable(log, 'outer', [1])) {
      for (var innerValue of iterable(log, 'inner', [1])) {
        {
          await using item = resource(log, 'continue');
          yield innerValue;
          log.push('continue-outer');
          continue outer;
        }
      }
    }
    log.push('after-loop');
    yield 'done';
  })();
  await it.next();
  pending = it.next();
  log.push('after-next');
  assert.compareArray(
    log,
    ['continue-outer', 'dispose-continue-start', 'after-next'],
    'labeled continue parks while unwinding the crossed inner loop'
  );
  result = await pending;
  assert.sameValue(result.value, 'done', 'continue resumes at the outer target');
  assert.sameValue(result.done, false, 'continue does not complete the generator');
  assert.compareArray(
    log,
    [
      'continue-outer',
      'dispose-continue-start',
      'after-next',
      'dispose-continue-end',
      'close-inner',
      'after-loop'
    ],
    'loop control resumes once after disposal and IteratorClose'
  );
  await it.return();

  log = [];
  it = (async function* () {
    try {
      outer: for (var outerValue of iterable(log, 'outer-error', [1])) {
        for (var innerValue of iterable(log, 'inner-error', [1])) {
          {
            await using item = {
              [Symbol.asyncDispose]() {
                log.push('dispose-error-start');
                return Promise.reject('dispose-error');
              }
            };
            yield innerValue;
            log.push('continue-error');
            continue outer;
          }
        }
      }
      log.push('wrong-target');
    } catch (error) {
      log.push('caught-' + error);
      yield 'caught';
    }
  })();
  await it.next();
  pending = it.next();
  log.push('after-next');
  assert.compareArray(
    log,
    ['continue-error', 'dispose-error-start', 'after-next'],
    'loop control parks before a rejecting disposer settles'
  );
  result = await pending;
  assert.sameValue(result.value, 'caught', 'the disposer error replaces loop control');
  assert.sameValue(result.done, false, 'the enclosing catch resumes the generator');
  assert.compareArray(
    log,
    [
      'continue-error',
      'dispose-error-start',
      'after-next',
      'close-inner-error',
      'close-outer-error',
      'caught-dispose-error'
    ],
    'replacement throw closes crossed loops and does not resume the stale target'
  );
  await it.return();
});
