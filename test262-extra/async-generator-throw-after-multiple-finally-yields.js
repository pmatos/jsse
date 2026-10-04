/*---
description: >
  A throw intercepted by a finally that yields more than once is not
  delivered until every yield in that finally has been resumed; the pending
  throw must not misfire on the resume that reaches the second (or later)
  yield.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  Evaluation of a try-finally statement evaluates the Finally clause for
  every completion of its try Block. Suspension points inside the Finally
  clause are ordinary yields of that clause's own evaluation; the throw the
  Finally clause is running on behalf of is not observable until the Finally
  clause itself completes.
flags: [async]
includes: [asyncHelpers.js]
features: [async-iteration]
---*/

async function* g() {
  try {
    throw 'E';
  } finally {
    yield 1;
    yield 2;
  }
}

async function main() {
  var it = g();
  assert.sameValue((await it.next()).value, 1, 'first yield inside the finally');
  assert.sameValue((await it.next()).value, 2, 'second yield inside the finally');

  var thrown;
  try {
    await it.next();
  } catch (e) {
    thrown = e;
  }
  assert.sameValue(thrown, 'E', 'the pending throw is delivered only after both yields resume');
}

asyncTest(main);
