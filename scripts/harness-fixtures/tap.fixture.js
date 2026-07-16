// Self-test for the describe/it/before/after TAP runner in
// node-test-harness.js. Validated on jsse alone (the harness is inert on Node);
// run-harness-selftest.sh asserts the exact summary line.
//
// Covers: nested suites, definition-order execution, before/after (once per
// suite) and beforeEach/afterEach (per test, parent chain), async it bodies,
// the test() alias, Jest-style test.each tables, and — via one deliberate
// throw — failure detection.
//
// Expected summary: PASS: 6  FAIL: 1  TOTAL: 7

var order = [];

describe("outer", function () {
  before(function () { order.push("before-outer"); });
  beforeEach(function () { order.push("beforeEach-outer"); });
  afterEach(function () { order.push("afterEach-outer"); });
  after(function () { order.push("after-outer"); });

  it("passes synchronously", function () {
    if (1 + 1 !== 2) throw new Error("math is broken");
  });

  it("passes asynchronously", async function () {
    await new Promise(function (r) { setTimeout(r, 0); });
  });

  it("fails as expected", function () {
    throw new Error("deliberate failure");
  });

  describe("inner", function () {
    beforeEach(function () { order.push("beforeEach-inner"); });
    it("nested test passes", function () {
      // beforeEach chain runs outermost -> innermost.
      var seen = order.slice(-2);
      if (seen[0] !== "beforeEach-outer" || seen[1] !== "beforeEach-inner") {
        throw new Error("beforeEach ordering wrong: " + order.join(","));
      }
    });
  });
});

test("top-level test() alias runs last (definition order)", function () {
  if (order.indexOf("before-outer") === -1) {
    throw new Error("outer suite did not run before root test");
  }
});

test.each([
  [1, 2, 3],
  [2, 3, 5],
])("test.each row %# adds %i and %i", function (a, b, expected) {
  if (a + b !== expected) throw new Error("table arguments were not forwarded");
});
