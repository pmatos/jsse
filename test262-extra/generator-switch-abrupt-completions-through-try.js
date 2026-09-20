/*---
description: >
  A throw from the discriminant or a case test of a switch statement whose body
  contains a yield is delivered to an enclosing catch or finally clause of the
  generator.
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
includes: [compareArray.js]
features: [generators]
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

function* discriminantThrowsCaught() {
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

var iter = discriminantThrowsCaught();
var result = iter.next();
assert.sameValue(result.value, 'caught:boom', 'discriminant throw reaches catch');
assert.sameValue(result.done, false);
assert.sameValue(iter.next().value, 'after', 'generator resumes after the catch');
assert.sameValue(iter.next().done, true);

var caseLog = [];
function log(name) {
  caseLog.push(name);
  return name;
}

function* caseTestThrowsCaught() {
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

assert.compareArray(
  [...caseTestThrowsCaught()],
  ['caught:boom', 'after'],
  'case test throw reaches catch and no case or default body runs'
);
assert.compareArray(
  caseLog,
  ['a'],
  'case tests after the throwing test are not evaluated'
);

var finallyLog = [];
function* discriminantThrowsThroughFinally() {
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

iter = discriminantThrowsThroughFinally();
assert.throws(Test262Error, function () {
  iter.next();
}, 'discriminant throw escapes after finally');
assert.compareArray(finallyLog, ['cleanup'], 'finally ran before the throw escaped');
result = iter.next();
assert.sameValue(result.value, undefined, 'generator is completed after the throw');
assert.sameValue(result.done, true);

var orderLog = [];
function* caseTestThrowsCatchAndFinally() {
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

assert.compareArray(
  [...caseTestThrowsCatchAndFinally()],
  ['caught:boom', 'after'],
  'catch handles the case test throw and the generator continues'
);
assert.compareArray(
  orderLog,
  ['catch', 'finally', 'outer finally'],
  'catch, finally, and outer finally run in order'
);

function* discriminantThrowsUnhandled() {
  switch (thrower()) {
    case 1:
      yield 'z';
      break;
  }
  yield 'after';
}

iter = discriminantThrowsUnhandled();
assert.throws(Test262Error, function () {
  iter.next();
}, 'unhandled discriminant throw escapes next()');
assert.sameValue(iter.next().done, true, 'generator is completed after an unhandled throw');

function* caseTestThrowsUnhandled() {
  switch (0) {
    case thrower():
      yield 'z';
      break;
  }
  yield 'after';
}

iter = caseTestThrowsUnhandled();
assert.throws(Test262Error, function () {
  iter.next();
}, 'unhandled case test throw escapes next()');
assert.sameValue(iter.next().done, true, 'generator is completed after an unhandled throw');

var closeLog = [];
function* forOfSwitchThrowsCaught() {
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

assert.compareArray(
  [...forOfSwitchThrowsCaught()],
  ['caught:boom', 'after'],
  'switch throw inside a for-of body is caught outside the loop'
);
assert.compareArray(
  closeLog,
  ['return', 'catch'],
  'the for-of iterator is closed exactly once before the catch body runs'
);

var uncaughtCloseLog = [];
function* forOfSwitchThrowsUnhandled() {
  for (var x of iterableWithReturnLog(uncaughtCloseLog)) {
    switch (0) {
      case thrower():
        yield 'z';
        break;
    }
  }
}

iter = forOfSwitchThrowsUnhandled();
assert.throws(Test262Error, function () {
  iter.next();
});
assert.compareArray(
  uncaughtCloseLog,
  ['return'],
  'the for-of iterator is closed exactly once when the throw escapes'
);
assert.sameValue(iter.next().done, true);
