// QUnit's default autostart runs tests after synchronous registration without
// requiring QUnit.load() or QUnit.start().
//
// Expected summary: PASS: 1  FAIL: 0  TOTAL: 1

QUnit.test("default autostart", function (assert) {
  assert.ok(true, "registered test ran");
});
