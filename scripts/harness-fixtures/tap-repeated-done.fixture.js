// Self-test for repeated completion callbacks in the describe/it TAP runner in
// node-test-harness.js. Validated on jsse alone (the harness is inert on Node);
// run-harness-selftest.sh asserts the exact summary line.
//
// A callback-style test that invokes done() more than once must fail even when
// the first call succeeded. The following test proves the duplicate failure is
// contained and does not stop the rest of the suite.
//
// Expected summary: PASS: 1  FAIL: 2  TOTAL: 3

describe("repeated done callbacks", function () {
  it("fails when done is called twice", function (done) {
    done();
    done();
  });

  it("fails when the second done carries an error", function (done) {
    done();
    done(new Error("second completion must not be ignored"));
  });

  it("continues with later tests", function () {});
});
