/*---
description: >
  The promise returned by an async generator's next() stays reachable while a
  collection runs inside the generator body it is about to settle.
esid: sec-asyncgeneratorenqueue
info: |
  AsyncGeneratorEnqueue appends an AsyncGeneratorRequest, whose [[Capability]]
  holds the promise and its resolving functions, to [[AsyncGeneratorQueue]],
  and %AsyncGeneratorPrototype%.next returns that capability's [[Promise]].
  The request stays in the queue while the body runs, so a collection that
  fires inside the body must not reclaim the promise or its resolving
  functions. jsse keeps the queue in the scheduler and must root it there
  (issue #679: the arena recycled the promise's id and next() returned an
  unrelated object).
flags: [async]
features: [async-iteration, host-gc-required]
---*/

async function* g() {
  $262.gc();
  var junk = [];
  for (var i = 0; i < 64; i++) junk.push({ i: i, s: "x" + i });
  yield 42;
}

g()
  .next()
  .then(function (r) {
    assert.sameValue(r.value, 42, "the yielded value is delivered");
    assert.sameValue(r.done, false, "the iterator result is not done");
  })
  .then($DONE, $DONE);
