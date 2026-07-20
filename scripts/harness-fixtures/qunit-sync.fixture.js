// The opt-in synchronous QUnit mode runs a wholly synchronous suite inside
// QUnit.start(), avoiding the engine's bounded async-drain window for large
// corpora such as Moment. Existing suites keep the asynchronous default.
//
// Expected summary: PASS: 2  FAIL: 2  TOTAL: 4

var order = [];
var finished = false;

QUnit.config.autostart = false;
QUnit.config.sync = true;
QUnit.module("sync", {
  beforeEach: function () {
    order.push("beforeEach");
  },
  afterEach: function () {
    order.push("afterEach");
  },
});
QUnit.test("runs without yielding", function (assert) {
  order.push("test");
  assert.equal(order.join(","), "beforeEach,test", "hook order");
  assert.ok(true, "second assertion");
});
QUnit.module("sync guard", {
  before: function (assert) {
    assert.async();
  },
});
QUnit.test("rejects incomplete async work", function (assert) {
  assert.async();
});
QUnit.done(function () {
  finished = true;
  if (order.join(",") !== "beforeEach,test,afterEach") {
    throw new Error("synchronous QUnit hook order was wrong: " + order.join(","));
  }
});

QUnit.start();
if (!finished) {
  throw new Error("QUnit.config.sync start returned before the suite finished");
}
QUnit.test("not registered after start", function (assert) {
  assert.ok(false);
});
