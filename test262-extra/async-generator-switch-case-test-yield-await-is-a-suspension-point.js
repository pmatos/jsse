/*---
description: >
  yield and await expressions in the test of a switch case clause are suspension
  points of the async generator.
esid: sec-runtime-semantics-caseblockevaluation
info: |
  Runtime Semantics: CaseBlockEvaluation

  CaseClauseIsSelected ( C, input )

  2. Let exprRef be ? Evaluation of the Expression of C.
  3. Let clauseSelector be ? GetValue(exprRef).
  4. Return IsStrictlyEqual(input, clauseSelector).

  yield and await are valid AssignmentExpressions in a case selector, so they
  suspend the async generator in the middle of CaseBlockEvaluation. The value
  sent to next() (or the awaited value) is the selector. Abrupt completions
  injected at the suspension point are delivered to the enclosing try statement
  (sec-try-statement-runtime-semantics-evaluation).
includes: [compareArray.js]
flags: [async]
features: [async-iteration]
---*/

var log = [];

async function* yieldSelector() {
  switch (1) {
    case yield 'q':
      return 'matched';
    case await 2:
      return 'awaited';
    default:
      return 'default';
  }
}

async function* awaitAfterYield() {
  switch (2) {
    case yield 'q':
      return 'first';
    case await 2:
      return 'second';
    default:
      return 'default';
  }
}

async function* throwAtSelector() {
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

async function* returnAtSelector() {
  try {
    switch (1) {
      case yield 'q':
        break;
    }
  } finally {
    log.push('finally');
  }
}

async function* forAwaitBody() {
  var trace = [];
  for await (var v of [0, 1, 2, 3]) {
    switch (v) {
      case yield 'sel' + v:
        trace.push('match' + v);
        continue;
      case (await 1):
        trace.push('one');
        break;
      case 2:
        trace.push('two');
        break;
      default:
        trace.push('default');
    }
    trace.push('after' + v);
  }
  return trace;
}

var it = yieldSelector();
it.next()
  .then(function (r) {
    assert.sameValue(r.value, 'q', 'the selector yield produces its operand');
    assert.sameValue(r.done, false, 'the generator is suspended at the selector');
    return it.next(1);
  })
  .then(function (r) {
    assert.sameValue(r.value, 'matched', 'the sent value selects the first clause');
    assert.sameValue(r.done, true, 'completes');
    it = yieldSelector();
    return it.next().then(function () {
      return it.next('1');
    });
  })
  .then(function (r) {
    assert.sameValue(r.value, 'default', 'strict equality: "1" is not 1');
    it = awaitAfterYield();
    return it.next().then(function () {
      return it.next(0);
    });
  })
  .then(function (r) {
    assert.sameValue(r.value, 'second', 'await selector after a yield selector');
    it = throwAtSelector();
    return it.next().then(function (first) {
      assert.sameValue(first.value, 'q', 'throw: suspended at selector');
      return it.throw('E');
    });
  })
  .then(function (r) {
    assert.sameValue(r.value, 'caught:E', 'throw() at the selector is caught by the enclosing catch');
    assert.compareArray(log, [], 'throw: no clause body or later selector runs');
    return it.next();
  })
  .then(function (r) {
    assert.sameValue(r.value, 'end', 'throw: generator continues after catch');
    log = [];
    it = returnAtSelector();
    return it.next().then(function () {
      return it.return('R');
    });
  })
  .then(function (r) {
    assert.sameValue(r.value, 'R', 'return: completion value');
    assert.sameValue(r.done, true, 'return: done');
    assert.compareArray(log, ['finally'], 'return() at the selector runs the enclosing finally');
    it = forAwaitBody();
    return it.next();
  })
  .then(function (r) {
    assert.sameValue(r.value, 'sel0', 'for await: first selector yield');
    return it.next(0);
  })
  .then(function (r) {
    assert.sameValue(r.value, 'sel1', 'for await: match continues the loop');
    return it.next(99);
  })
  .then(function (r) {
    assert.sameValue(r.value, 'sel2', 'for await: await selector matches 1');
    return it.next(99);
  })
  .then(function (r) {
    assert.sameValue(r.value, 'sel3', 'for await: plain selector matches 2');
    return it.next(99);
  })
  .then(function (r) {
    assert.sameValue(r.done, true, 'for await: loop finishes');
    assert.compareArray(r.value, ['match0', 'one', 'after1', 'two', 'after2', 'default', 'after3'], 'for await: trace');
  })
  .then($DONE, $DONE);
