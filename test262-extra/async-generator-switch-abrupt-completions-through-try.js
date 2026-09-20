/*---
description: >
  A throw from the discriminant or a case test of a switch statement whose body
  contains a yield is delivered to an enclosing catch or finally clause of the
  async generator.
esid: sec-switch-statement-runtime-semantics-evaluation
info: |
  SwitchStatement : switch ( Expression ) CaseBlock

  1. Let exprRef be ? Evaluation of Expression.
  2. Let switchValue be ? GetValue(exprRef).

  CaseClauseIsSelected ( C, input )

  2. Let exprRef be ? Evaluation of the Expression of C.

  CaseBlockEvaluation propagates the abrupt completion and stops evaluating
  further case tests. TryStatement Evaluation then routes the throw completion
  through the enclosing catch and finally clauses.

  An unhandled throw completion leaves the generator body, which disposes the
  resources of its function-level `using` declarations before the request
  rejects.
flags: [async]
includes: [compareArray.js]
features: [async-iteration, explicit-resource-management]
---*/

function thrower() {
  throw new Test262Error('boom');
}

function iterableWithReturnLog(events) {
  var index = 0;
  return {
    [Symbol.iterator]: function () {
      return {
        next: function () {
          return { value: index++, done: false };
        },
        return: function () {
          events.push('return');
          return { value: undefined, done: true };
        },
      };
    },
  };
}

async function collect(generator) {
  var values = [];
  for (;;) {
    var result = await generator.next();
    if (result.done) {
      return values;
    }
    values.push(result.value);
  }
}

async function rejection(promise) {
  try {
    await promise;
  } catch (error) {
    return error;
  }
  throw new Test262Error('expected the promise to reject');
}

async function* discriminantThrowsCaught() {
  try {
    switch (thrower()) {
      case 1:
        yield 'z';
        break;
    }
  } catch (e) {
    yield 'caught:' + e.message;
  }
  yield 'after';
}

var caseLog = [];
function log(name) {
  caseLog.push(name);
  return name;
}

async function* caseTestThrowsCaught() {
  try {
    switch (0) {
      case log('a'):
      case thrower():
      case log('never'):
        yield 'z';
        break;
      default:
        yield 'default';
    }
  } catch (e) {
    yield 'caught:' + e.message;
  }
  yield 'after';
}

var finallyLog = [];
async function* discriminantThrowsThroughFinally() {
  try {
    switch (thrower()) {
      case 1:
        yield 'z';
        break;
    }
  } finally {
    finallyLog.push('cleanup');
  }
  yield 'after';
}

var orderLog = [];
async function* caseTestThrowsCatchAndFinally() {
  try {
    try {
      switch (0) {
        case thrower():
          yield 'z';
          break;
      }
    } catch (e) {
      orderLog.push('catch');
      yield 'caught:' + e.message;
    } finally {
      orderLog.push('finally');
    }
    yield 'after';
  } finally {
    orderLog.push('outer finally');
  }
}

async function* discriminantThrowsUnhandled() {
  switch (thrower()) {
    case 1:
      yield 'z';
      break;
  }
  yield 'after';
}

async function* caseTestThrowsUnhandled() {
  switch (0) {
    case thrower():
      yield 'z';
      break;
  }
  yield 'after';
}

var closeLog = [];
async function* forOfSwitchThrowsCaught() {
  try {
    for (var x of iterableWithReturnLog(closeLog)) {
      switch (thrower()) {
        case 1:
          yield 'z';
          break;
      }
    }
  } catch (e) {
    closeLog.push('catch');
    yield 'caught:' + e.message;
  }
  yield 'after';
}

var uncaughtCloseLog = [];
async function* forOfSwitchThrowsUnhandled() {
  for (var x of iterableWithReturnLog(uncaughtCloseLog)) {
    switch (0) {
      case thrower():
        yield 'z';
        break;
    }
  }
}

var disposeLog = [];
async function* discriminantThrowsDisposesResources() {
  using resource = {
    [Symbol.dispose]() {
      disposeLog.push('dispose');
    },
  };
  switch (thrower()) {
    case 1:
      yield 'z';
      break;
  }
}

var caseDisposeLog = [];
async function* caseTestThrowsDisposesResources() {
  using resource = {
    [Symbol.dispose]() {
      caseDisposeLog.push('dispose');
    },
  };
  switch (0) {
    case thrower():
      yield 'z';
      break;
  }
}

async function run() {
  var iter = discriminantThrowsCaught();
  var result = await iter.next();
  assert.sameValue(result.value, 'caught:boom', 'discriminant throw reaches catch');
  assert.sameValue(result.done, false);
  result = await iter.next();
  assert.sameValue(result.value, 'after', 'generator resumes after the catch');
  assert.sameValue((await iter.next()).done, true);

  assert.compareArray(
    await collect(caseTestThrowsCaught()),
    ['caught:boom', 'after'],
    'case test throw reaches catch and no case or default body runs'
  );
  assert.compareArray(
    caseLog,
    ['a'],
    'case tests after the throwing test are not evaluated'
  );

  iter = discriminantThrowsThroughFinally();
  var error = await rejection(iter.next());
  assert.sameValue(error instanceof Test262Error, true, 'discriminant throw rejects after finally');
  assert.compareArray(finallyLog, ['cleanup'], 'finally ran before the request rejected');
  result = await iter.next();
  assert.sameValue(result.value, undefined, 'generator is completed after the throw');
  assert.sameValue(result.done, true);

  assert.compareArray(
    await collect(caseTestThrowsCatchAndFinally()),
    ['caught:boom', 'after'],
    'catch handles the case test throw and the generator continues'
  );
  assert.compareArray(
    orderLog,
    ['catch', 'finally', 'outer finally'],
    'catch, finally, and outer finally run in order'
  );

  iter = discriminantThrowsUnhandled();
  error = await rejection(iter.next());
  assert.sameValue(error instanceof Test262Error, true, 'unhandled discriminant throw rejects');
  assert.sameValue((await iter.next()).done, true, 'generator is completed after an unhandled throw');

  iter = caseTestThrowsUnhandled();
  error = await rejection(iter.next());
  assert.sameValue(error instanceof Test262Error, true, 'unhandled case test throw rejects');
  assert.sameValue((await iter.next()).done, true, 'generator is completed after an unhandled throw');

  assert.compareArray(
    await collect(forOfSwitchThrowsCaught()),
    ['caught:boom', 'after'],
    'switch throw inside a for-of body is caught outside the loop'
  );
  assert.compareArray(
    closeLog,
    ['return', 'catch'],
    'the for-of iterator is closed exactly once before the catch body runs'
  );

  iter = forOfSwitchThrowsUnhandled();
  error = await rejection(iter.next());
  assert.sameValue(error instanceof Test262Error, true);
  assert.compareArray(
    uncaughtCloseLog,
    ['return'],
    'the for-of iterator is closed exactly once when the throw escapes'
  );
  assert.sameValue((await iter.next()).done, true);

  iter = discriminantThrowsDisposesResources();
  error = await rejection(iter.next());
  assert.sameValue(error instanceof Test262Error, true, 'unhandled discriminant throw rejects');
  assert.compareArray(disposeLog, ['dispose'], 'the using resource is disposed when the discriminant throws');
  assert.sameValue((await iter.next()).done, true);

  iter = caseTestThrowsDisposesResources();
  error = await rejection(iter.next());
  assert.sameValue(error instanceof Test262Error, true, 'unhandled case test throw rejects');
  assert.compareArray(caseDisposeLog, ['dispose'], 'the using resource is disposed when a case test throws');
  assert.sameValue((await iter.next()).done, true);
}

run().then($DONE, $DONE);
