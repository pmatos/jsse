// Self-test for Mocha-compatible exclusive test selection in the describe/it
// TAP runner. Validated through the harness's emitted summary.
//
// Expected summary: PASS: 1  FAIL: 0  TOTAL: 1

it.only("runs the focused test", function () {});

// Direct exclusive tests take precedence over child suites at the same level,
// even when the child suite is itself marked exclusive.
describe.only("does not run an exclusive sibling suite", function () {
  it("does not run", function () {
    throw new Error("exclusive suite ran despite direct it.only");
  });
});
