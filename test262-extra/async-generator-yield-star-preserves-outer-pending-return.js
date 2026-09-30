/*---
description: >
  A return pending on a try context survives delegation in an async
  generator: a yield* in the finally block delegates to another iterator, but
  the return value parked on the try context does not live in the
  delegation's own snapshot, so it is not lost when the delegated iterator is
  drained.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  Evaluation of a try-finally statement evaluates the Finally clause for
  every completion of its try Block; a yield* inside that Finally clause
  suspends the same generator activation, without discarding the completion
  the Finally clause is running to restore.
flags: [async]
includes: [asyncHelpers.js]
features: [async-iteration]
---*/

async function* g() {
  try {
    yield 0;
  } finally {
    yield* [1, 2];
  }
}

async function main() {
  var it = g();
  assert.sameValue((await it.next()).value, 0, 'suspended inside the try block');
  var result = await it.return(42);
  assert.sameValue(result.value, 1, 'return() enters the finally, delegating to the array iterator');
  assert.sameValue(result.done, false, 'not done while the delegation is still in progress');

  result = await it.next();
  assert.sameValue(result.value, 2, 'second delegated value');
  assert.sameValue(result.done, false, 'still not done after the second delegated value');

  result = await it.next();
  assert.sameValue(result.value, 42, 'the pending return survives the yield* delegation');
  assert.sameValue(result.done, true, 'the generator completes with the pending return value');
}

asyncTest(main);
