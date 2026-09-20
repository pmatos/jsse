/*---
description: >
  A throw from the test expression of an if, while, do-while, or for statement,
  or from the test or left operand of a conditional or logical expression, is
  delivered to an enclosing catch or finally clause of the async generator even
  when the statement's body or the other operand contains a yield.
esid: sec-if-statement-runtime-semantics-evaluation
info: |
  IfStatement : if ( Expression ) Statement else Statement

  1. Let exprRef be ? Evaluation of Expression.
  2. Let exprValue be ToBoolean(? GetValue(exprRef)).

  The test expressions of do-while (sec-runtime-semantics-dowhileloopevaluation),
  while (sec-runtime-semantics-whileloopevaluation), and for
  (sec-forbodyevaluation) statements, the test of the conditional operator
  (sec-conditional-operator-runtime-semantics-evaluation) and the left operand
  of &&, ||, and ?? (sec-binary-logical-operators-runtime-semantics-evaluation)
  are all evaluated with `?`, so a throw completion propagates out of the
  statement or expression.

  TryStatement : try Block Catch Finally
  (sec-try-statement-runtime-semantics-evaluation)

  1. Let B be Completion(Evaluation of Block).
  2. If B is a throw completion, let C be Completion(CatchClauseEvaluation of
     Catch with argument B.[[Value]]).
  4. Let F be Completion(Evaluation of Finally).

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

async function* ifThrowsCaught() {
  try {
    if (thrower()) {
      yield 'z';
    }
  } catch (e) {
    yield 'caught-if:' + e.message;
  }
  yield 'after';
}

async function* ifElseThrowsCaught() {
  try {
    if (thrower()) {
      yield 'z';
    } else {
      yield 'else';
    }
  } catch (e) {
    yield 'caught-if-else:' + e.message;
  }
}

async function* labeledIfThrowsCaught() {
  try {
    label: if (thrower()) {
      yield 'z';
    }
  } catch (e) {
    yield 'caught-labeled-if:' + e.message;
  }
}

async function* whileThrowsCaught() {
  try {
    while (thrower()) {
      yield 'z';
    }
  } catch (e) {
    yield 'caught-while:' + e.message;
  }
}

async function* doWhileThrowsCaught() {
  var pass = 0;
  try {
    do {
      yield 'body' + pass++;
    } while (thrower());
  } catch (e) {
    yield 'caught-do-while:' + e.message;
  }
}

async function* forThrowsCaught() {
  try {
    for (; thrower(); ) {
      yield 'z';
    }
  } catch (e) {
    yield 'caught-for:' + e.message;
  }
}

var finallyLog = [];
async function* ifThrowsThroughFinally() {
  try {
    if (thrower()) {
      yield 'z';
    }
  } finally {
    finallyLog.push('cleanup');
  }
  yield 'after';
}

var whileFinallyLog = [];
async function* whileThrowsThroughFinally() {
  try {
    while (thrower()) {
      yield 'z';
    }
  } finally {
    whileFinallyLog.push('cleanup');
  }
}

var nestedLog = [];
async function* nestedFinallyInsideCatch() {
  try {
    try {
      if (thrower()) {
        yield 'z';
      }
    } finally {
      nestedLog.push('inner finally');
    }
  } catch (e) {
    nestedLog.push('outer catch');
    yield 'caught:' + e.message;
  }
}

async function* conditionalThrowsCaught() {
  try {
    thrower() ? yield 'z' : 2;
  } catch (e) {
    yield 'caught-conditional:' + e.message;
  }
}

async function* logicalOrThrowsCaught() {
  try {
    thrower() || (yield 'z');
  } catch (e) {
    yield 'caught-or:' + e.message;
  }
}

async function* logicalAndThrowsCaught() {
  try {
    thrower() && (yield 'z');
  } catch (e) {
    yield 'caught-and:' + e.message;
  }
}

async function* coalesceThrowsCaught() {
  try {
    thrower() ?? (yield 'z');
  } catch (e) {
    yield 'caught-coalesce:' + e.message;
  }
}

async function* throwsAfterEarlierYield() {
  var attempts = 0;
  try {
    yield 'first';
    if (attempts++ === 0 ? thrower() : false) {
      yield 'z';
    }
  } catch (e) {
    yield 'caught-after-yield:' + e.message;
  }
  yield 'after';
}

async function* caughtThenAwaits() {
  try {
    if (thrower()) {
      yield 'z';
    }
  } catch (e) {
    var awaited = await Promise.resolve('awaited:' + e.message);
    yield awaited;
  }
  yield 'after';
}

var closeLog = [];
async function* forOfConditionThrowsCaught() {
  try {
    for (var x of iterableWithReturnLog(closeLog)) {
      if (thrower()) {
        yield 'z';
      }
    }
  } catch (e) {
    closeLog.push('catch');
    yield 'caught:' + e.message;
  }
  yield 'after';
}

var uncaughtCloseLog = [];
async function* forOfConditionThrowsUnhandled() {
  for (var x of iterableWithReturnLog(uncaughtCloseLog)) {
    while (thrower()) {
      yield 'z';
    }
  }
}

async function* ifThrowsUnhandled() {
  if (thrower()) {
    yield 'z';
  }
  yield 'after';
}

var disposeLog = [];
async function* ifThrowsDisposesResources() {
  using resource = {
    [Symbol.dispose]() {
      disposeLog.push('dispose');
    },
  };
  if (thrower()) {
    yield 'z';
  }
}

var loopDisposeLog = [];
async function* whileThrowsDisposesResources() {
  using resource = {
    [Symbol.dispose]() {
      loopDisposeLog.push('dispose');
    },
  };
  while (thrower()) {
    yield 'z';
  }
}

async function run() {
  var iter = ifThrowsCaught();
  var result = await iter.next();
  assert.sameValue(result.value, 'caught-if:boom', 'if test throw reaches catch');
  assert.sameValue(result.done, false);
  result = await iter.next();
  assert.sameValue(result.value, 'after', 'generator resumes after the catch');
  assert.sameValue((await iter.next()).done, true);

  assert.compareArray(
    await collect(ifElseThrowsCaught()),
    ['caught-if-else:boom'],
    'if/else test throw reaches catch and neither branch runs'
  );

  assert.compareArray(
    await collect(labeledIfThrowsCaught()),
    ['caught-labeled-if:boom'],
    'labeled if test throw reaches catch'
  );

  assert.compareArray(
    await collect(whileThrowsCaught()),
    ['caught-while:boom'],
    'while test throw reaches catch'
  );

  assert.compareArray(
    await collect(doWhileThrowsCaught()),
    ['body0', 'caught-do-while:boom'],
    'do-while test throw reaches catch after the first body pass'
  );

  assert.compareArray(
    await collect(forThrowsCaught()),
    ['caught-for:boom'],
    'for test throw reaches catch'
  );

  iter = ifThrowsThroughFinally();
  var error = await rejection(iter.next());
  assert.sameValue(error instanceof Test262Error, true, 'if test throw rejects after finally');
  assert.compareArray(finallyLog, ['cleanup'], 'finally ran exactly once before the request rejected');
  result = await iter.next();
  assert.sameValue(result.value, undefined, 'generator is completed after the throw');
  assert.sameValue(result.done, true);

  iter = whileThrowsThroughFinally();
  error = await rejection(iter.next());
  assert.sameValue(error instanceof Test262Error, true, 'while test throw rejects after finally');
  assert.compareArray(whileFinallyLog, ['cleanup'], 'finally body ran before the request rejected');
  assert.sameValue((await iter.next()).done, true);

  assert.compareArray(
    await collect(nestedFinallyInsideCatch()),
    ['caught:boom'],
    'outer catch handles the throw after the inner finally'
  );
  assert.compareArray(
    nestedLog,
    ['inner finally', 'outer catch'],
    'inner finally runs before the outer catch'
  );

  assert.compareArray(
    await collect(conditionalThrowsCaught()),
    ['caught-conditional:boom'],
    'conditional operator test throw reaches catch'
  );

  assert.compareArray(
    await collect(logicalOrThrowsCaught()),
    ['caught-or:boom'],
    '|| left operand throw reaches catch'
  );

  assert.compareArray(
    await collect(logicalAndThrowsCaught()),
    ['caught-and:boom'],
    '&& left operand throw reaches catch'
  );

  assert.compareArray(
    await collect(coalesceThrowsCaught()),
    ['caught-coalesce:boom'],
    '?? left operand throw reaches catch'
  );

  assert.compareArray(
    await collect(throwsAfterEarlierYield()),
    ['first', 'caught-after-yield:boom', 'after'],
    'a condition throw on a resumed state reaches catch'
  );

  assert.compareArray(
    await collect(caughtThenAwaits()),
    ['awaited:boom', 'after'],
    'the generator continues to await after the caught condition throw'
  );

  assert.compareArray(
    await collect(forOfConditionThrowsCaught()),
    ['caught:boom', 'after'],
    'condition throw inside a for-of body is caught outside the loop'
  );
  assert.compareArray(
    closeLog,
    ['return', 'catch'],
    'the for-of iterator is closed exactly once before the catch body runs'
  );

  iter = forOfConditionThrowsUnhandled();
  error = await rejection(iter.next());
  assert.sameValue(error instanceof Test262Error, true);
  assert.compareArray(
    uncaughtCloseLog,
    ['return'],
    'the for-of iterator is closed exactly once when the throw escapes'
  );
  assert.sameValue((await iter.next()).done, true);

  iter = ifThrowsUnhandled();
  error = await rejection(iter.next());
  assert.sameValue(error instanceof Test262Error, true, 'unhandled if test throw rejects');
  assert.sameValue((await iter.next()).done, true, 'generator is completed after an unhandled throw');

  iter = ifThrowsDisposesResources();
  error = await rejection(iter.next());
  assert.sameValue(error instanceof Test262Error, true, 'unhandled if test throw rejects');
  assert.compareArray(disposeLog, ['dispose'], 'the using resource is disposed when the if test throws');
  assert.sameValue((await iter.next()).done, true);

  iter = whileThrowsDisposesResources();
  error = await rejection(iter.next());
  assert.sameValue(error instanceof Test262Error, true, 'unhandled while test throw rejects');
  assert.compareArray(loopDisposeLog, ['dispose'], 'the using resource is disposed when the while test throws');
  assert.sameValue((await iter.next()).done, true);
}

run().then($DONE, $DONE);
