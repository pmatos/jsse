// Self-test for the describe/it/before/after TAP runner in
// node-test-harness.js. Validated on jsse alone (the harness is inert on Node);
// run-harness-selftest.sh asserts the exact summary line.
//
// Covers: nested suites, definition-order execution, before/after (once per
// suite) and beforeEach/afterEach (per test, parent chain), async it bodies,
// the test() alias, Jest-style test.each tables, skipped suites (xdescribe),
// and — via deliberate throw/done(error) failures — failure detection.
//
// Expected summary: PASS: 9  FAIL: 2  TOTAL: 11

var order = [];

describe("outer", function () {
  before(function () { order.push("before-outer"); });
  before(function (done) {
    setTimeout(function () {
      order.push("before-outer-done");
      done();
    }, 0);
  });
  beforeEach(function () { order.push("beforeEach-outer"); });
  beforeEach(function (done) {
    setTimeout(function () {
      order.push("beforeEach-outer-done");
      done();
    }, 0);
  });
  afterEach(function () { order.push("afterEach-outer"); });
  afterEach(function (done) {
    setTimeout(function () {
      order.push("afterEach-outer-done");
      done();
    }, 0);
  });
  after(function () { order.push("after-outer"); });
  after(function (done) {
    setTimeout(function () {
      order.push("after-outer-done");
      done();
    }, 0);
  });

  it("passes synchronously", function () {
    if (1 + 1 !== 2) throw new Error("math is broken");
  });

  it("passes asynchronously", async function () {
    await new Promise(function (r) { setTimeout(r, 0); });
  });

  it("waits for a done callback", function (done) {
    setTimeout(function () {
      order.push("done-test-complete");
      done();
    }, 0);
  });

  it("fails as expected", function () {
    throw new Error("deliberate failure");
  });

  it("fails when done receives an error", function (done) {
    setTimeout(function () {
      done(new Error("deliberate done failure"));
    }, 0);
  });

  describe("inner", function () {
    beforeEach(function () { order.push("beforeEach-inner"); });
    it("nested test passes", function () {
      // beforeEach chain runs outermost -> innermost.
      var seen = order.slice(-3);
      if (
        seen[0] !== "beforeEach-outer" ||
        seen[1] !== "beforeEach-outer-done" ||
        seen[2] !== "beforeEach-inner"
      ) {
        throw new Error("beforeEach ordering wrong: " + order.join(","));
      }
    });
  });
});

// xdescribe is Mocha's alias for describe.skip: its callback still runs so
// nested tests register (and count in the total) as skipped, but no hook or
// body executes.
xdescribe("xdescribe suite", function () {
  before(function () { throw new Error("xdescribe suite hook ran"); });
  it("registers its tests without running them", function () {
    throw new Error("xdescribe suite test ran");
  });
  describe("nested inside xdescribe", function () {
    it("inherits the skipped state", function () {
      throw new Error("nested xdescribe test ran");
    });
  });
});

test("top-level test() alias runs last (definition order)", function () {
  if (order.indexOf("before-outer") === -1) {
    throw new Error("outer suite did not run before root test");
  }
  if (order.indexOf("before-outer-done") === -1) {
    throw new Error("done-style before hook did not complete");
  }
  if (order.indexOf("beforeEach-outer-done") === -1) {
    throw new Error("done-style beforeEach hook did not complete");
  }
  if (order.indexOf("afterEach-outer-done") === -1) {
    throw new Error("done-style afterEach hook did not complete");
  }
  if (order.indexOf("after-outer-done") === -1) {
    throw new Error("done-style after hook did not complete");
  }
  if (order.indexOf("done-test-complete") === -1) {
    throw new Error("done-style test did not complete");
  }
});

test.each([
  [1, 2, 3],
  [2, 3, 5],
])("test.each row %# adds %i and %i", function (a, b, expected) {
  if (a + b !== expected) throw new Error("table arguments were not forwarded");
});
