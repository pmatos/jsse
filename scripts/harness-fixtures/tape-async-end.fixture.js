// Self-test for callback-style asynchronous tape completion. The shared
// harness is inert on Node, so run-harness-selftest.sh validates this fixture
// on JSSE alone.
//
// Expected summary: PASS: 8  FAIL: 1  TOTAL: 9

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

tape("plan fulfillment completes without end", function (t) {
  t.plan(1);
  setTimeout(function () {
    t.ok(true, "deferred planned assertion");
  }, 0);
});

tape("deferred end completes without a plan", function (t) {
  setTimeout(function () {
    t.ok(true, "deferred unplanned assertion");
    t.end();
  }, 0);
});

tape("plan declared after its assertion completes", function (t) {
  t.ok(true, "assertion before plan");
  t.plan(1);
});

tape("planned skip fulfills the plan", function (t) {
  t.plan(1);
  t.skip("planned skip");
});

tape("subtest-only parent auto-ends", function (t) {
  t.test("synchronous child", function (st) {
    st.ok(true, "child assertion");
    st.end();
  });
});

tape("returned promise auto-ends", async function (t) {
  t.ok(true, "promise assertion");
});

tape("synchronous throw auto-ends", function () {
  throw new Error("intentional fixture failure");
});
