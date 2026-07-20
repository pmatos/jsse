// Self-test for the QUnit adapter in node-test-harness.js.
//
// Unlike scripts/shim-fixtures/ (which cross-check against Node's native
// Buffer/TextEncoder), the harness is jsse-only by design — it is inert on Node,
// where a suite's own framework runs instead. So this fixture is validated on
// jsse alone, by run-harness-selftest.sh asserting the exact summary line it
// prints. It exercises every assert method, the tricky deepEqual cases, async
// tests, expect() planning, raises, and — via one deliberately failing
// assertion — that failures are detected and counted (not just passes).
//
// Expected summary: PASS: 24  FAIL: 1  TOTAL: 25

QUnit.module("primitives");
QUnit.test("equality asserts", function (assert) {
  assert.expect(6);
  assert.ok(1, "ok truthy");
  assert.notOk(0, "notOk falsy");
  assert.equal(1, "1", "loose equal");
  assert.notEqual(1, 2, "loose notEqual");
  assert.strictEqual(2, 2, "strict equal");
  assert.notStrictEqual(1, "1", "strict notEqual");
});

QUnit.module("deepEqual");
QUnit.test("structural equality edge cases", function (assert) {
  assert.expect(9);
  assert.deepEqual({ a: [1, 2], b: { c: 3 } }, { a: [1, 2], b: { c: 3 } }, "nested");
  assert.deepEqual([, "x"], [undefined, "x"], "QUnit ignores sparse hole ownership");
  assert.deepEqual([NaN], [NaN], "NaN equals NaN structurally");
  assert.deepEqual(new Date(0), new Date(0), "Date by value");
  assert.deepEqual(/x/gi, /x/gi, "RegExp by source+flags");
  assert.deepEqual(new Map([[1, 2]]), new Map([[1, 2]]), "Map");
  assert.deepEqual(new Set([1, 2]), new Set([2, 1]), "Set unordered");
  assert.notDeepEqual({ a: 1 }, { a: 2 }, "differing values");
  var a = {}; a.self = a;
  var b = {}; b.self = b;
  assert.deepEqual(a, b, "cyclic");
});

QUnit.module("throws");
QUnit.test("raises forms", function (assert) {
  assert.expect(3);
  assert.raises(function () { throw new TypeError("x"); }, TypeError, "constructor match");
  assert.raises(function () { throw new Error("y"); }, "any throw");
  assert.raises(function () { null.x; }, TypeError, "native TypeError");
});

QUnit.module("async");
QUnit.test("async token completes the test", function (assert) {
  assert.expect(2);
  assert.ok(true, "sync assertion before async");
  var done = assert.async();
  setTimeout(function () {
    assert.ok(true, "assertion after timeout");
    done();
  }, 0);
});

QUnit.module("failure detection");
QUnit.test("a failing assertion is counted as FAIL", function (assert) {
  assert.expect(2);
  assert.ok(true, "this passes");
  assert.strictEqual(1, 2, "this deliberately fails");
});

var nestedOrder = [];
QUnit.module("nested parent", function (hooks) {
  hooks.before(function () {
    nestedOrder.push("parent before");
  });
  hooks.beforeEach(function () {
    nestedOrder.push("parent beforeEach");
  });
  hooks.afterEach(function () {
    nestedOrder.push("parent afterEach");
  });
  hooks.after(function () {
    nestedOrder.push("parent after");
  });

  QUnit.test("parent test before child", function (assert) {
    nestedOrder.push("parent test before child");
    assert.deepEqual(
      nestedOrder,
      ["parent before", "parent beforeEach", "parent test before child"],
      "parent hooks start at the first test"
    );
  });

  QUnit.module("nested child", function (childHooks) {
    childHooks.before(function () {
      nestedOrder.push("child before");
    });
    childHooks.beforeEach(function () {
      nestedOrder.push("child beforeEach");
    });
    childHooks.afterEach(function () {
      nestedOrder.push("child afterEach");
    });
    childHooks.after(function () {
      nestedOrder.push("child after");
    });

    QUnit.test("child inherits parent per-test hooks", function (assert) {
      nestedOrder.push("child test");
      assert.deepEqual(
        nestedOrder.slice(-4),
        [
          "child before",
          "parent beforeEach",
          "child beforeEach",
          "child test",
        ],
        "beforeEach hooks run outermost to innermost"
      );
    });
  });

  QUnit.test("parent test after child", function (assert) {
    nestedOrder.push("parent test after child");
    assert.deepEqual(
      nestedOrder.slice(-5),
      [
        "child afterEach",
        "parent afterEach",
        "child after",
        "parent beforeEach",
        "parent test after child",
      ],
      "child hooks close before execution returns to the parent"
    );
  });
});

QUnit.done(function () {
  var expected = [
    "parent before",
    "parent beforeEach",
    "parent test before child",
    "parent afterEach",
    "child before",
    "parent beforeEach",
    "child beforeEach",
    "child test",
    "child afterEach",
    "parent afterEach",
    "child after",
    "parent beforeEach",
    "parent test after child",
    "parent afterEach",
    "parent after",
  ];
  if (nestedOrder.join("|") !== expected.join("|")) {
    throw new Error("nested QUnit hook order wrong: " + nestedOrder.join(","));
  }
});

QUnit.config.noglobals = true;
QUnit.load();
QUnit.start();
