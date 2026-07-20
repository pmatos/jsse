// Self-test for global it.only selection across nested suite branches and a
// skipped suite. Retained entries stay in definition order.
//
// Expected summary: PASS: 4  FAIL: 0  TOTAL: 4

var order = [];

describe("first branch", function () {
  beforeEach(function () {
    order.push("first-hook");
  });

  it("does not run an ordinary sibling", function () {
    throw new Error("unfocused test ran");
  });

  it.only("runs the first focused test", function () {
    order.push("first-test");
  });

  it.only("runs the second focused test", function () {
    if (order.join(",") !== "first-hook,first-test,first-hook") {
      throw new Error("focused tests lost definition order");
    }
  });
});

describe("second branch", function () {
  describe("nested path", function () {
    it.only("runs focus in another branch", function () {});
  });

  describe("ordinary nested sibling", function () {
    it("does not run", function () {
      throw new Error("unfocused nested suite ran");
    });
  });
});

describe("skipped branch", function () {
  describe.skip("skipped focused suite", function () {
    before(function () {
      throw new Error("skipped focused hook ran");
    });

    it.only("reports the focused test as skipped", function () {
      throw new Error("skipped focused test ran");
    });

    it("drops an unfocused skipped sibling", function () {
      throw new Error("unfocused skipped test ran");
    });
  });
});

describe("unfocused branch", function () {
  before(function () {
    throw new Error("unfocused branch hook ran");
  });

  it("does not run", function () {
    throw new Error("unfocused branch test ran");
  });
});
