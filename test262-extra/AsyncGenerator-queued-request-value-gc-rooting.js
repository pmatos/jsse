/*---
description: >
  Queued async generator requests keep their send values and capabilities
  reachable across a collection that runs while an earlier request is being
  serviced.
esid: sec-asyncgeneratorenqueue
info: |
  Each call to next() appends an AsyncGeneratorRequest to
  [[AsyncGeneratorQueue]]; its [[Completion]] carries the sent value and its
  [[Capability]] the promise to settle (AsyncGeneratorYield resolves the
  capability of the request at the head of the queue). A request that is still
  queued is reachable only through the queue, so a collection between two
  resumptions must not reclaim its value or its promise (issue #679).
flags: [async]
features: [async-iteration, host-gc-required]
---*/

var log = [];

async function* h() {
  var a = yield 1;
  $262.gc();
  var junk = [];
  for (var i = 0; i < 64; i++) junk.push({ i: i, s: "x" + i });
  log.push(a.tag);
  var b = yield 2;
  log.push(b.tag);
  return 3;
}

var it = h();
it.next();
it.next({ tag: "A" });
it.next({ tag: "B" });
it
  .next()
  .then(function (r) {
    assert.sameValue(r.value, undefined, "a request on a completed generator has no value");
    assert.sameValue(r.done, true, "the generator is done");
    assert.sameValue(log.join(), "A,B", "every queued send value survived collection");
  })
  .then($DONE, $DONE);
