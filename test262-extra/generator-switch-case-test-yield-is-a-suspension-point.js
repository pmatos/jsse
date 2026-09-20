/*---
description: >
  A yield expression in the test of a switch case clause is a suspension point
  of the generator.
esid: sec-runtime-semantics-caseblockevaluation
info: |
  Runtime Semantics: CaseBlockEvaluation

  CaseClauseIsSelected ( C, input )

  2. Let exprRef be ? Evaluation of the Expression of C.
  3. Let clauseSelector be ? GetValue(exprRef).
  4. Return IsStrictlyEqual(input, clauseSelector).

  A yield expression is a valid AssignmentExpression in a case selector, so
  evaluating it suspends the generator in the middle of CaseBlockEvaluation and
  resumes with the value sent to next().

  Case selectors are evaluated in source order and evaluation stops at the first
  selected clause. The discriminant is evaluated once, before any selector.
  Abrupt completions injected at the suspension point are delivered to the
  enclosing try statement (sec-try-statement-runtime-semantics-evaluation).
includes: [compareArray.js]
flags: [async]
features: [generators, explicit-resource-management]
---*/

function* single() {
  switch (1) {
    case (yield 5, 1):
      return 'x';
  }
}

var it = single();
var first = it.next();
assert.sameValue(first.value, 5, 'the selector yield produces its operand');
assert.sameValue(first.done, false, 'the generator is suspended at the selector');
var second = it.next();
assert.sameValue(second.value, 'x', 'the selected clause runs after the selector resumes');
assert.sameValue(second.done, true, 'the generator completes');

assert.compareArray([...single()], [5], 'spreading yields only the selector value');

function drive(it, sent) {
  var out = [];
  var r = it.next();
  var i = 0;
  while (!r.done) {
    out.push(r.value);
    r = it.next(sent[i++]);
  }
  return { yielded: out, result: r.value };
}

// Selectors are evaluated in source order and evaluation stops at the first match.
var log = [];
function* order() {
  switch (2) {
    case (log.push('t1'), yield 'a', 1):
      log.push('body1');
      break;
    case (log.push('t2'), yield 'b', 2):
      log.push('body2');
      break;
    case (log.push('t3'), yield 'c', 3):
      log.push('body3');
      break;
  }
  return 'done';
}
var r = drive(order(), []);
assert.compareArray(r.yielded, ['a', 'b'], 'order: later selectors are not evaluated');
assert.sameValue(r.result, 'done', 'order: completion value');
assert.compareArray(log, ['t1', 't2', 'body2'], 'order: side effects');

// default in the middle: tests on both sides of it run in source order first.
log = [];
function* defaultInMiddle(disc) {
  switch (disc) {
    case (log.push('t1'), yield 'a', 1):
      log.push('body1');
    default:
      log.push('bodyD');
    case (log.push('t3'), yield 'b', 3):
      log.push('body3');
      break;
    case (log.push('t4'), yield 'c', 4):
      log.push('body4');
  }
  return 'done';
}
r = drive(defaultInMiddle(9), []);
assert.compareArray(r.yielded, ['a', 'b', 'c'], 'default: every selector runs before default');
assert.compareArray(log, ['t1', 't3', 't4', 'bodyD', 'body3'], 'default: falls back then falls through');
log = [];
r = drive(defaultInMiddle(3), []);
assert.compareArray(r.yielded, ['a', 'b'], 'default: match after default skips default');
assert.compareArray(log, ['t1', 't3', 'body3'], 'default: matching clause after default');
log = [];
r = drive(defaultInMiddle(1), []);
assert.compareArray(r.yielded, ['a'], 'default: match before default');
assert.compareArray(log, ['t1', 'body1', 'bodyD', 'body3'], 'default: fallthrough from first clause');

// The value sent to next() is the selector, compared with strict equality.
function* sent() {
  switch (1) {
    case yield 'q':
      return 'matched';
    default:
      return 'default';
  }
}
r = drive(sent(), [1]);
assert.sameValue(r.result, 'matched', 'sent value 1 selects the clause');
r = drive(sent(), ['1']);
assert.sameValue(r.result, 'default', 'sent string "1" does not select a numeric 1');
function* sentNaN() {
  switch (NaN) {
    case yield 'q':
      return 'matched';
    default:
      return 'default';
  }
}
r = drive(sentNaN(), [NaN]);
assert.sameValue(r.result, 'default', 'NaN never matches');

// yield* as a selector.
function* inner() {
  yield 'i1';
  return 2;
}
function* delegated() {
  switch (2) {
    case yield* inner():
      return 'matched';
    default:
      return 'default';
  }
}
r = drive(delegated(), []);
assert.compareArray(r.yielded, ['i1'], 'yield* selector delegates');
assert.sameValue(r.result, 'matched', 'yield* selector result is the delegate return value');

// Suspending and non-suspending selectors in one switch.
log = [];
function* mixed() {
  switch (3) {
    case (log.push('plain1'), 1):
      return 'one';
    case (log.push('susp'), yield 'y', 2):
      return 'two';
    case (log.push('plain2'), 3):
      return 'three';
  }
}
r = drive(mixed(), []);
assert.compareArray(r.yielded, ['y'], 'mixed: yields');
assert.sameValue(r.result, 'three', 'mixed: plain selector after a suspending one');
assert.compareArray(log, ['plain1', 'susp', 'plain2'], 'mixed: order');

// The discriminant is evaluated once, before any selector.
var x;
function* discOnce() {
  x = 1;
  switch (x) {
    case (x = 2, yield 'y', 2):
      return 'changed';
    case 1:
      return 'original';
  }
}
r = drive(discOnce(), []);
assert.sameValue(r.result, 'original', 'a selector cannot change the captured discriminant');

// Suspending discriminant and suspending selectors.
function* both() {
  switch (yield 'd') {
    case yield 's1':
      return 's1';
    case yield 's2':
      return 's2';
  }
}
r = drive(both(), ['v', 'x', 'v']);
assert.compareArray(r.yielded, ['d', 's1', 's2'], 'both: yield order');
assert.sameValue(r.result, 's2', 'both: second selector matches the discriminant');

// break, labeled break and continue from a clause of a lowered switch.
function* controlFlow() {
  var trace = [];
  outer: for (var i = 0; i < 4; i++) {
    switch (i) {
      case (yield 'sel' + i, 0):
        trace.push('zero');
        break;
      case 1:
        trace.push('one');
        continue;
      case 2:
        trace.push('two');
        break outer;
      default:
        trace.push('default');
    }
    trace.push('after' + i);
  }
  return trace;
}
r = drive(controlFlow(), []);
assert.compareArray(r.yielded, ['sel0', 'sel1', 'sel2'], 'control flow: selector runs per iteration');
assert.compareArray(r.result, ['zero', 'after0', 'one', 'two'], 'control flow: break/continue/labeled break');

// throw() at a selector suspension is delivered to an enclosing catch.
log = [];
function* throwAtSelector() {
  try {
    switch (1) {
      case yield 'q':
        log.push('body');
        break;
      case (log.push('later'), 2):
        break;
    }
  } catch (e) {
    yield 'caught:' + e;
  }
  return 'end';
}
var it = throwAtSelector();
assert.sameValue(it.next().value, 'q', 'throw: suspended at selector');
assert.sameValue(it.throw('E').value, 'caught:E', 'throw: enclosing catch runs');
assert.sameValue(it.next().value, 'end', 'throw: generator continues after catch');
assert.compareArray(log, [], 'throw: no clause body or later selector runs');

// return() at a selector suspension runs the enclosing finally.
log = [];
function* returnAtSelector() {
  try {
    switch (1) {
      case yield 'q':
        break;
    }
  } finally {
    log.push('finally');
  }
}
it = returnAtSelector();
it.next();
var ret = it.return('R');
assert.sameValue(ret.value, 'R', 'return: completion value');
assert.sameValue(ret.done, true, 'return: done');
assert.compareArray(log, ['finally'], 'return: finally runs');

// A throw from a non-suspending selector of a lowered switch reaches the enclosing catch.
log = [];
function thrower() {
  log.push('thrower');
  throw 'boom';
}
function* plainSelectorThrows() {
  try {
    switch (1) {
      case (yield 'a', 2):
        break;
      case thrower():
        break;
      case (log.push('never'), 3):
        break;
    }
  } catch (e) {
    yield 'caught:' + e;
  } finally {
    log.push('finally');
  }
  return 'end';
}
r = drive(plainSelectorThrows(), []);
assert.compareArray(r.yielded, ['a', 'caught:boom'], 'selector throw: delivered to catch');
assert.sameValue(r.result, 'end', 'selector throw: completion');
assert.compareArray(log, ['thrower', 'finally'], 'selector throw: later selectors are not evaluated');

// An unhandled selector throw in a lowered switch disposes function-level using resources.
var disposeLog = [];
function throwsTest262Error() {
  throw new Test262Error('boom');
}
function* selectorThrowDisposesResources() {
  using resource = {
    [Symbol.dispose]() {
      disposeLog.push('dispose');
    },
  };
  switch (0) {
    case (yield 'a', 1):
      break;
    case throwsTest262Error():
      break;
  }
}
it = selectorThrowDisposesResources();
assert.sameValue(it.next().value, 'a', 'using: suspended at selector');
assert.throws(Test262Error, function () {
  it.next();
}, 'using: unhandled selector throw escapes next()');
assert.compareArray(disposeLog, ['dispose'], 'using: resource is disposed');
assert.sameValue(it.next().done, true, 'using: generator is closed');

$DONE();
