// Self-test for Mocha-compatible describe.only selection in the describe/it
// TAP runner. A focused suite includes all descendants and preserves skip.
//
// Expected summary: PASS: 5  FAIL: 0  TOTAL: 5

describe.only("focused suite", function () {
  var ready = false;

  before(function () {
    ready = true;
  });

  it("runs a direct test", function () {
    if (!ready) throw new Error("focused suite hook did not run");
  });

  describe("ordinary nested suite", function () {
    it("runs a nested test", function () {
      if (!ready) throw new Error("ancestor hook did not run");
    });
  });

  it.skip("retains a skipped test", function () {
    throw new Error("skipped focused test ran");
  });
});

describe.only("focused suite narrowed by nested focus", function () {
  it("does not run a direct test", function () {
    throw new Error("outer unfocused test ran");
  });

  describe("ordinary nested sibling", function () {
    it("does not run", function () {
      throw new Error("ordinary nested sibling ran");
    });
  });

  describe.only("nested focused suite", function () {
    it("runs all tests in the nested focus", function () {});
    it("runs another nested focused test", function () {});
  });
});

describe("ordinary sibling suite", function () {
  before(function () {
    throw new Error("unfocused suite hook ran");
  });

  it("does not run", function () {
    throw new Error("unfocused suite test ran");
  });
});
