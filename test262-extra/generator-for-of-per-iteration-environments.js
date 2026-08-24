/*---
description: >
  Generator for-of loops preserve fresh lexical bindings across suspension.
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
info: |
  ForIn/OfBodyEvaluation creates a new declarative environment for every
  lexical iteration, evaluates the loop body in that environment, and restores
  the environment that was active before the loop afterwards.

  Suspending a generator must retain the active iteration environment so that
  closures created by different iterations capture different bindings.
flags: [async]
includes: [compareArray.js]
features: [generators, async-iteration, destructuring-binding]
---*/

function* syncGenerator() {
  for (const value of [1, 2]) {
    yield function () { return value; };
  }

  for (let [value] of [[3], [4]]) {
    yield function () { return value; };
  }
}

var syncClosures = [...syncGenerator()];
assert.compareArray(
  syncClosures.map(function (closure) { return closure(); }),
  [1, 2, 3, 4],
  'sync generator iterations have distinct lexical bindings'
);

async function* asyncGenerator() {
  for (const value of [5, 6]) {
    yield function () { return value; };
  }
}

(async function () {
  var asyncClosures = [];
  for await (const closure of asyncGenerator()) {
    asyncClosures.push(closure);
  }

  assert.compareArray(
    asyncClosures.map(function (closure) { return closure(); }),
    [5, 6],
    'async generator iterations have distinct lexical bindings'
  );
})().then($DONE, $DONE);
