// Self-test for the exclusive table form test.only.each(table)(name, fn).
//
// The reported regression: replacing the old `it.only = it` alias with a bare
// function dropped the `.each` helper, so test.only.each(...) threw
// `TypeError: test.only.each is not a function` at registration. This fixture
// guards the *semantics*, not just absence-of-throw: each row must register as
// a FOCUSED test, so the ordinary siblings are dropped and only the two rows
// run. A naive fix that restored `.each` but registered rows as ordinary tests
// would leave the siblings in place — they would run, throw, and this fixture
// would fail with FAIL: 2.
//
// Expected summary: PASS: 2  FAIL: 0  TOTAL: 2

describe("test.only.each exclusivity", function () {
  test("ordinary sibling before the table is dropped", function () {
    throw new Error("unfocused sibling ran (before .each)");
  });

  test.only.each([[1], [2]])("focused row %d runs", function (n) {
    if (n !== 1 && n !== 2) {
      throw new Error("unexpected row value: " + n);
    }
  });

  test("ordinary sibling after the table is dropped", function () {
    throw new Error("unfocused sibling ran (after .each)");
  });
});
