// Self-test for callback-style asynchronous tape completion. The shared
// harness is inert on Node, so run-harness-selftest.sh validates this fixture
// on JSSE alone.
//
// Expected summary: PASS: 2  FAIL: 0  TOTAL: 2

var tape = globalThis.__tape;

tape("waits for deferred plan fulfillment", function (t) {
  t.plan(1);
  setTimeout(function () {
    t.equal(1, 1, "deferred assertion");
    t.end();
  }, 10);
});

tape("runs the synchronous sibling afterward", function (t) {
  t.ok(true, "synchronous assertion");
  t.end();
});
