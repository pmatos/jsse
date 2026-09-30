/*---
description: >
  A completion pending on a try context is restored only if the finally
  completes normally. An abrupt completion escaping the finally (a new
  throw, a new return, or a break/continue that leaves it) replaces the
  pending completion outright, for every combination of throw and return. A
  throw caught inside the finally does not escape it, so the pending
  completion survives; an uncaught throw does escape, and replaces it.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  TryStatement : try Block Finally
    ...
    3. Let f be Completion(Evaluation of Finally).
    4. If f.[[Type]] is normal, set f to B.
    5. Return ? UpdateEmpty(f, undefined).

  Step 4 restores the try/catch completion B only when the Finally clause
  itself completes normally (step 3). Any abrupt f from the Finally clause
  is returned as-is in step 5, replacing B.
includes: [compareArray.js]
features: [generators]
---*/

function values(gen) {
  var it = gen();
  var first = it.next();
  assert.sameValue(first.value, 0, 'suspended inside the try block');
  var result;
  try {
    result = it.next();
    return { completion: 'return', value: result.value };
  } catch (e) {
    return { completion: 'throw', value: e };
  }
}

var outcome = values(function* () {
  try {
    yield 0;
    return 'A';
  } finally {
    return 'B';
  }
});
assert.sameValue(outcome.completion, 'return', 'return-return: finally return replaces try return');
assert.sameValue(outcome.value, 'B', 'return-return: value');

outcome = values(function* () {
  try {
    yield 0;
    return 'A';
  } finally {
    throw 'B';
  }
});
assert.sameValue(outcome.completion, 'throw', 'return-throw: finally throw replaces try return');
assert.sameValue(outcome.value, 'B', 'return-throw: value');

outcome = values(function* () {
  try {
    yield 0;
    throw 'A';
  } finally {
    return 'B';
  }
});
assert.sameValue(outcome.completion, 'return', 'throw-return: finally return replaces try throw');
assert.sameValue(outcome.value, 'B', 'throw-return: value');

outcome = values(function* () {
  try {
    yield 0;
    throw 'A';
  } finally {
    throw 'B';
  }
});
assert.sameValue(outcome.completion, 'throw', 'throw-throw: finally throw replaces try throw');
assert.sameValue(outcome.value, 'B', 'throw-throw: value');

var log = [];
outcome = values(function* () {
  try {
    yield 0;
    throw 'OUTER';
  } finally {
    try {
      throw 'INNER';
    } catch (e) {
      log.push('caught ' + e);
    }
  }
});
assert.sameValue(outcome.completion, 'throw', 'caught-inside: a throw caught inside the finally does not replace the outer one');
assert.sameValue(outcome.value, 'OUTER', 'caught-inside: the outer throw is preserved');
assert.compareArray(log, ['caught INNER'], 'caught-inside: the inner throw was actually caught');
