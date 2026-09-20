/*---
description: >
  A throw from the expression of a return statement in an async generator is
  delivered to an enclosing catch or finally clause of the generator.
esid: sec-return-statement-runtime-semantics-evaluation
info: |
  ReturnStatement : return Expression ;

  1. Let exprRef be ? Evaluation of Expression.
  2. Let exprValue be ? GetValue(exprRef).
  3. If GetGeneratorKind() is async, set exprValue to ? Await(exprValue).

  The `?` in steps 1 and 2 propagates a throw completion out of the return
  statement, and TryStatement Evaluation
  (sec-try-statement-runtime-semantics-evaluation) then routes it through the
  enclosing catch and finally clauses.

  An unhandled throw completion leaves the generator body, which disposes the
  resources of its function-level `using` declarations before the request
  rejects (sec-asyncgeneratorstart).
flags: [async]
includes: [compareArray.js]
features: [async-iteration, explicit-resource-management]
---*/

function thrower() {
  throw new Test262Error('boom');
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

async function* returnThrowsCaught() {
  try {
    yield 'first';
    return thrower();
  } catch (e) {
    yield 'caught:' + e.message;
  }
  yield 'after';
}

var finallyLog = [];
async function* returnThrowsThroughFinally() {
  try {
    yield 'first';
    return thrower();
  } finally {
    finallyLog.push('cleanup');
  }
}

var orderLog = [];
async function* returnThrowsCatchAndFinally() {
  try {
    try {
      yield 'first';
      return thrower();
    } catch (e) {
      orderLog.push('catch');
      yield 'caught:' + e.message;
    } finally {
      orderLog.push('finally');
    }
  } finally {
    orderLog.push('outer finally');
  }
}

async function* returnThrowsUnhandled() {
  yield 'first';
  return thrower();
}

var disposeLog = [];
async function* returnThrowsDisposesResources() {
  using resource = {
    [Symbol.dispose]() {
      disposeLog.push('dispose');
    },
  };
  yield 'first';
  return thrower();
}

async function run() {
  assert.compareArray(
    await collect(returnThrowsCaught()),
    ['first', 'caught:boom', 'after'],
    'return expression throw reaches catch and the generator continues'
  );

  var iter = returnThrowsThroughFinally();
  assert.sameValue((await iter.next()).value, 'first');
  var error = await rejection(iter.next());
  assert.sameValue(error instanceof Test262Error, true, 'return expression throw rejects after finally');
  assert.compareArray(finallyLog, ['cleanup'], 'finally ran before the request rejected');
  var result = await iter.next();
  assert.sameValue(result.value, undefined, 'generator is completed after the throw');
  assert.sameValue(result.done, true);

  assert.compareArray(
    await collect(returnThrowsCatchAndFinally()),
    ['first', 'caught:boom'],
    'catch handles the return expression throw'
  );
  assert.compareArray(
    orderLog,
    ['catch', 'finally', 'outer finally'],
    'catch, finally, and outer finally run in order'
  );

  iter = returnThrowsUnhandled();
  assert.sameValue((await iter.next()).value, 'first');
  error = await rejection(iter.next());
  assert.sameValue(error instanceof Test262Error, true, 'unhandled return expression throw rejects');
  assert.sameValue((await iter.next()).done, true, 'generator is completed after an unhandled throw');

  iter = returnThrowsDisposesResources();
  assert.sameValue((await iter.next()).value, 'first');
  error = await rejection(iter.next());
  assert.sameValue(error instanceof Test262Error, true, 'unhandled return expression throw rejects');
  assert.compareArray(disposeLog, ['dispose'], 'the using resource is disposed when the return expression throws');
  assert.sameValue((await iter.next()).done, true);
}

run().then($DONE, $DONE);
