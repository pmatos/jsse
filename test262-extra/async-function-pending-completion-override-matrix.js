/*---
description: >
  A completion pending on a plain async function's try context is restored
  only if the finally completes normally. An abrupt completion escaping the
  finally (a new throw, a new return, or a break/continue that leaves it)
  replaces the pending completion outright, for every combination of throw
  and return. A throw caught inside the finally does not escape it, so the
  pending completion survives; an uncaught throw does escape, and replaces
  it. A break/continue that leaves the finalizer replaces the earlier
  completion too.
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
flags: [async]
includes: [compareArray.js, asyncHelpers.js]
features: [async-functions]
---*/

async function outcome(promise) {
  try {
    return { completion: 'fulfilled', value: await promise };
  } catch (e) {
    return { completion: 'rejected', value: e };
  }
}

async function main() {
  var result = await outcome(
    (async function () {
      try {
        await 0;
        return 'A';
      } finally {
        return 'B';
      }
    })()
  );
  assert.sameValue(result.completion, 'fulfilled', 'return-return: finally return replaces try return');
  assert.sameValue(result.value, 'B', 'return-return: value');

  result = await outcome(
    (async function () {
      try {
        await 0;
        return 'A';
      } finally {
        throw 'B';
      }
    })()
  );
  assert.sameValue(result.completion, 'rejected', 'return-throw: finally throw replaces try return');
  assert.sameValue(result.value, 'B', 'return-throw: value');

  result = await outcome(
    (async function () {
      try {
        await 0;
        throw 'A';
      } finally {
        return 'B';
      }
    })()
  );
  assert.sameValue(result.completion, 'fulfilled', 'throw-return: finally return replaces try throw');
  assert.sameValue(result.value, 'B', 'throw-return: value');

  result = await outcome(
    (async function () {
      try {
        await 0;
        throw 'A';
      } finally {
        throw 'B';
      }
    })()
  );
  assert.sameValue(result.completion, 'rejected', 'throw-throw: finally throw replaces try throw');
  assert.sameValue(result.value, 'B', 'throw-throw: value');

  var log = [];
  result = await outcome(
    (async function () {
      try {
        await 0;
        throw 'OUTER';
      } finally {
        try {
          await 0;
          throw 'INNER';
        } catch (e) {
          log.push('caught ' + e);
        }
      }
    })()
  );
  assert.sameValue(
    result.completion,
    'rejected',
    'caught-inside: a throw caught inside the finally does not replace the outer one'
  );
  assert.sameValue(result.value, 'OUTER', 'caught-inside: the outer throw is preserved');
  assert.compareArray(log, ['caught INNER'], 'caught-inside: the inner throw was actually caught');

  var value = await (async function () {
    for (;;) {
      try {
        await 0;
        throw 'REPLACED';
      } finally {
        await 0;
        break;
      }
    }
  })();
  assert.sameValue(
    value,
    undefined,
    'loop-control-leaves: a break that leaves the finalizer replaces the pending throw'
  );
}

asyncTest(main);
