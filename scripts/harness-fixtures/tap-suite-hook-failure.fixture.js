// Self-test for suite-level (before all / after all) hook FAILURE containment in
// the describe/it TAP runner in node-test-harness.js. Validated on jsse alone
// (the harness is inert on Node); run-harness-selftest.sh asserts the exact
// summary line.
//
// A failing suite-level hook must be recorded as a single `not ok` and the run
// must continue — it must NOT propagate to the top-level harness catch, which
// would collapse every count to `PASS: 0  FAIL: 1  TOTAL: 1` and discard the
// real results of sibling suites. This is the suite-level analogue of the
// per-test beforeEach/afterEach containment exercised by tap.fixture.js.
//
// Covers: a done(error) `before all` (children skipped, siblings still run), a
// synchronously-throwing `before all` (non-callback path also contained), and a
// done(error) `after all` (its tests still count, the hook adds one failure). A
// timed-out hook rejects through the identical path, so it is not re-exercised
// here (it would cost ASYNC_TIMEOUT_MS of wall-clock).
//
// Expected summary: PASS: 2  FAIL: 3  TOTAL: 5

describe("suite-with-failing-before", function () {
  before(function (done) {
    setTimeout(function () {
      done(new Error("deliberate before-all failure"));
    }, 0);
  });

  // Both of these must be skipped: a failed `before all` leaves fixture state
  // unreliable, so the whole subtree (direct tests and nested suites) is skipped.
  it("must not run when before-all fails", function () {
    throw new Error("test body ran despite a failed before-all hook");
  });
  describe("nested under a failing before", function () {
    it("nested test must not run either", function () {
      throw new Error("nested test ran despite a failed before-all hook");
    });
  });
});

describe("sibling-suite", function () {
  // Proves the run continues after a prior suite's before-all failed.
  it("runs normally after a sibling's before-all failed", function () {
    if (1 + 1 !== 2) throw new Error("unreachable");
  });
});

describe("suite-with-throwing-before", function () {
  // Non-callback (synchronous throw) before-all is contained the same way.
  before(function () {
    throw new Error("deliberate synchronous before-all failure");
  });
  it("must not run when before-all throws", function () {
    throw new Error("test body ran despite a throwing before-all hook");
  });
});

describe("suite-with-failing-after", function () {
  after(function (done) {
    setTimeout(function () {
      done(new Error("deliberate after-all failure"));
    }, 0);
  });

  // The test itself passes and is counted; the after-all failure adds exactly
  // one more `not ok` without discarding this pass.
  it("passes before the after-all hook fails", function () {
    if (1 + 1 !== 2) throw new Error("unreachable");
  });
});
