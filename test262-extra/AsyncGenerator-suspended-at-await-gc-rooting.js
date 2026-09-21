/*---
description: >
  An async generator that only its pending request references stays reachable
  while it is suspended at an await, so the promise returned by next() still
  settles after a collection.
esid: sec-asyncgeneratorenqueue
info: |
  AsyncGeneratorEnqueue appends a request to [[AsyncGeneratorQueue]] and the
  generator resumes from the reaction job of the promise it awaits. While that
  request is outstanding the generator is reachable through the pending await,
  so a collection that runs between two resumptions must not reclaim it or
  strand the request's promise (issue #679). jsse captures the generator only
  inside native reaction handlers, so it must be rooted through its queue.
flags: [async]
features: [async-iteration, host-gc-required]
---*/

async function* g() {
  await null;
  await null;
  await null;
  yield 42;
}

g()
  .next()
  .then(function (r) {
    assert.sameValue(r.value, 42, "the yielded value is delivered");
    assert.sameValue(r.done, false, "the iterator result is not done");
  })
  .then($DONE, $DONE);

Promise.resolve().then(function () {
  $262.gc();
  $262.gc();
});
