/*---
description: >
  A throw from the test expression of an if, while, do-while, or for statement,
  or from the test or left operand of a conditional or logical expression, is
  delivered to an enclosing catch or finally clause of the generator even when
  the statement's body or the other operand contains a yield.
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
  resources of its function-level `using` declarations before the throw escapes
  (sec-generatorstart).
includes: [compareArray.js]
features: [generators, explicit-resource-management]
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

function* ifThrowsCaught() {
  try {
    if (thrower()) {
      yield 'z';
    }
  } catch (e) {
    yield 'caught-if:' + e.message;
  }
  yield 'after';
}

var iter = ifThrowsCaught();
var result = iter.next();
assert.sameValue(result.value, 'caught-if:boom', 'if test throw reaches catch');
assert.sameValue(result.done, false);
assert.sameValue(iter.next().value, 'after', 'generator resumes after the catch');
assert.sameValue(iter.next().done, true);

function* ifElseThrowsCaught() {
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

assert.compareArray(
  [...ifElseThrowsCaught()],
  ['caught-if-else:boom'],
  'if/else test throw reaches catch and neither branch runs'
);

function* labeledIfThrowsCaught() {
  try {
    label: if (thrower()) {
      yield 'z';
    }
  } catch (e) {
    yield 'caught-labeled-if:' + e.message;
  }
}

assert.compareArray(
  [...labeledIfThrowsCaught()],
  ['caught-labeled-if:boom'],
  'labeled if test throw reaches catch'
);

function* whileThrowsCaught() {
  try {
    while (thrower()) {
      yield 'z';
    }
  } catch (e) {
    yield 'caught-while:' + e.message;
  }
}

assert.compareArray(
  [...whileThrowsCaught()],
  ['caught-while:boom'],
  'while test throw reaches catch'
);

function* doWhileThrowsCaught() {
  var pass = 0;
  try {
    do {
      yield 'body' + pass++;
    } while (thrower());
  } catch (e) {
    yield 'caught-do-while:' + e.message;
  }
}

assert.compareArray(
  [...doWhileThrowsCaught()],
  ['body0', 'caught-do-while:boom'],
  'do-while test throw reaches catch after the first body pass'
);

function* forThrowsCaught() {
  try {
    for (; thrower(); ) {
      yield 'z';
    }
  } catch (e) {
    yield 'caught-for:' + e.message;
  }
}

assert.compareArray(
  [...forThrowsCaught()],
  ['caught-for:boom'],
  'for test throw reaches catch'
);

var finallyLog = [];
function* ifThrowsThroughFinally() {
  try {
    if (thrower()) {
      yield 'z';
    }
  } finally {
    finallyLog.push('cleanup');
  }
  yield 'after';
}

iter = ifThrowsThroughFinally();
assert.throws(Test262Error, function () {
  iter.next();
}, 'if test throw escapes after finally');
assert.compareArray(finallyLog, ['cleanup'], 'finally ran exactly once before the throw escaped');
result = iter.next();
assert.sameValue(result.value, undefined, 'generator is completed after the throw');
assert.sameValue(result.done, true);

var whileFinallyLog = [];
function* whileThrowsThroughFinally() {
  try {
    while (thrower()) {
      yield 'z';
    }
  } finally {
    whileFinallyLog.push('cleanup');
  }
}

iter = whileThrowsThroughFinally();
assert.throws(Test262Error, function () {
  iter.next();
}, 'while test throw escapes after finally');
assert.compareArray(whileFinallyLog, ['cleanup'], 'finally body ran before the throw escaped');
assert.sameValue(iter.next().done, true);

var nestedLog = [];
function* nestedFinallyInsideCatch() {
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

assert.compareArray(
  [...nestedFinallyInsideCatch()],
  ['caught:boom'],
  'outer catch handles the throw after the inner finally'
);
assert.compareArray(
  nestedLog,
  ['inner finally', 'outer catch'],
  'inner finally runs before the outer catch'
);

function* conditionalThrowsCaught() {
  try {
    thrower() ? yield 'z' : 2;
  } catch (e) {
    yield 'caught-conditional:' + e.message;
  }
}

assert.compareArray(
  [...conditionalThrowsCaught()],
  ['caught-conditional:boom'],
  'conditional operator test throw reaches catch'
);

function* logicalOrThrowsCaught() {
  try {
    thrower() || (yield 'z');
  } catch (e) {
    yield 'caught-or:' + e.message;
  }
}

assert.compareArray(
  [...logicalOrThrowsCaught()],
  ['caught-or:boom'],
  '|| left operand throw reaches catch'
);

function* logicalAndThrowsCaught() {
  try {
    thrower() && (yield 'z');
  } catch (e) {
    yield 'caught-and:' + e.message;
  }
}

assert.compareArray(
  [...logicalAndThrowsCaught()],
  ['caught-and:boom'],
  '&& left operand throw reaches catch'
);

function* coalesceThrowsCaught() {
  try {
    thrower() ?? (yield 'z');
  } catch (e) {
    yield 'caught-coalesce:' + e.message;
  }
}

assert.compareArray(
  [...coalesceThrowsCaught()],
  ['caught-coalesce:boom'],
  '?? left operand throw reaches catch'
);

function* throwsAfterEarlierYield() {
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

assert.compareArray(
  [...throwsAfterEarlierYield()],
  ['first', 'caught-after-yield:boom', 'after'],
  'a condition throw on a resumed state reaches catch'
);

function* yieldingFinallyAfterConditionThrow() {
  try {
    if (thrower()) {
      yield 'z';
    }
  } finally {
    yield 'finally-yield';
    finallyLog.push('finally-tail');
  }
}

finallyLog.length = 0;
iter = yieldingFinallyAfterConditionThrow();
result = iter.next();
assert.sameValue(result.value, 'finally-yield', 'finally runs and may yield');
assert.sameValue(result.done, false);
assert.throws(Test262Error, function () {
  iter.next();
}, 'the pending throw resumes after the finally body completes');
assert.compareArray(finallyLog, ['finally-tail'], 'the rest of the finally body ran');
assert.sameValue(iter.next().done, true);

var closeLog = [];
function* forOfConditionThrowsCaught() {
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

assert.compareArray(
  [...forOfConditionThrowsCaught()],
  ['caught:boom', 'after'],
  'condition throw inside a for-of body is caught outside the loop'
);
assert.compareArray(
  closeLog,
  ['return', 'catch'],
  'the for-of iterator is closed exactly once before the catch body runs'
);

var uncaughtCloseLog = [];
function* forOfConditionThrowsUnhandled() {
  for (var x of iterableWithReturnLog(uncaughtCloseLog)) {
    while (thrower()) {
      yield 'z';
    }
  }
}

iter = forOfConditionThrowsUnhandled();
assert.throws(Test262Error, function () {
  iter.next();
});
assert.compareArray(
  uncaughtCloseLog,
  ['return'],
  'the for-of iterator is closed exactly once when the throw escapes'
);
assert.sameValue(iter.next().done, true);

function* ifThrowsUnhandled() {
  if (thrower()) {
    yield 'z';
  }
  yield 'after';
}

iter = ifThrowsUnhandled();
assert.throws(Test262Error, function () {
  iter.next();
}, 'unhandled if test throw escapes next()');
assert.sameValue(iter.next().done, true, 'generator is completed after an unhandled throw');

var disposeLog = [];
function* ifThrowsDisposesResources() {
  using resource = {
    [Symbol.dispose]() {
      disposeLog.push('dispose');
    },
  };
  if (thrower()) {
    yield 'z';
  }
}

iter = ifThrowsDisposesResources();
assert.throws(Test262Error, function () {
  iter.next();
}, 'unhandled if test throw escapes next()');
assert.compareArray(disposeLog, ['dispose'], 'the using resource is disposed when the if test throws');
assert.sameValue(iter.next().done, true);

var loopDisposeLog = [];
function* whileThrowsDisposesResources() {
  using resource = {
    [Symbol.dispose]() {
      loopDisposeLog.push('dispose');
    },
  };
  while (thrower()) {
    yield 'z';
  }
}

iter = whileThrowsDisposesResources();
assert.throws(Test262Error, function () {
  iter.next();
}, 'unhandled while test throw escapes next()');
assert.compareArray(loopDisposeLog, ['dispose'], 'the using resource is disposed when the while test throws');
assert.sameValue(iter.next().done, true);

function* returnExpressionThrowsCaught() {
  try {
    yield 'first';
    return thrower();
  } catch (e) {
    yield 'caught-return:' + e.message;
  }
}

assert.compareArray(
  [...returnExpressionThrowsCaught()],
  ['first', 'caught-return:boom'],
  'a throw from a return expression reaches catch'
);
