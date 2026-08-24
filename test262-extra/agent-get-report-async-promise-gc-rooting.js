/*---
description: >
  A pending getReportAsync promise keeps its resolving functions reachable
  after its creating call frame returns and across a forced collection.
esid: sec-createresolvingfunctions
info: |
  CreateResolvingFunctions creates the resolve and reject functions that settle
  a promise. Deferred host work retains the corresponding PromiseCapability
  until it can call one of those functions.

  Regression test for issue #465: JSSE kept the host's resolve function on the
  evaluator's frame-scoped temporary-root stack. The enclosing eval_call frame
  truncated that stack when getReportAsync returned, so a collection reclaimed
  the resolve function and the report promise stayed pending forever.
flags: [async]
features: [host-gc-required]
---*/

function getPendingReport() {
  return $262.agent.getReportAsync();
}

var reportPromise = getPendingReport();

reportPromise
  .then(function (report) {
    assert.sameValue(report, "resolver survived collection");
  })
  .then($DONE, $DONE);

// The getReportAsync call frame has returned. Its promise remains reachable,
// but the host completion is the only owner of its resolving function.
$262.gc();

$262.agent.start(`
  $262.agent.report("resolver survived collection");
  $262.agent.leaving();
`);
