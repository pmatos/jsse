// Self-test for repeated completion callbacks in the describe/it TAP runner in
// node-test-harness.js. Validated on jsse alone (the harness is inert on Node);
// run-harness-selftest.sh asserts the exact summary line.
//
// A callback-style test that invokes done() more than once must fail even when
// the first call succeeded. The following test proves the duplicate failure is
// contained and does not stop the rest of the suite.
//
// Expected summary: PASS: 2  FAIL: 3  TOTAL: 5

describe("repeated done callbacks", function () {
  it("fails when done is called twice", function (done) {
    done();
    done();
  });

  it("fails when the second done carries an error", function (done) {
    done();
    done(new Error("second completion must not be ignored"));
  });

  it("fails when done is repeated after settlement", function (done) {
    done();
    setTimeout(done, 5);
  });

  // Keep the suite active long enough for the preceding test's late callback.
  it("waits while the late duplicate fires", function (done) {
    setTimeout(done, 20);
  });

  it("continues with later tests", function () {});
});
