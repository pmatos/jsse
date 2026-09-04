// QUnit.load() re-checks autostart after an earlier registration phase left it
// disabled. Deferring registration by one microtask ensures this specifically
// exercises load(), after the harness's initial autostart check has passed.
//
// Expected summary: PASS: 1  FAIL: 0  TOTAL: 1

QUnit.config.autostart = false;
Promise.resolve().then(function () {
  QUnit.test("load starts an autostart suite", function (assert) {
    assert.ok(true, "registered test ran");
  });
  QUnit.config.autostart = true;
  QUnit.load();
});
