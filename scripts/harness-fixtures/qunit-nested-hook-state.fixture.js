// Self-test for nested-module hook-state inheritance in the QUnit adapter of
// node-test-harness.js. Validated on jsse alone (the harness is inert on Node);
// run-harness-selftest.sh asserts the exact summary line.
//
// In QUnit, a nested module inherits its parent module's testEnvironment: state
// assigned to `this` in a parent `before` hook is the base context that a child
// module's own hooks and its tests see. Every case below was cross-checked
// against real qunit@2.26.0 — all three tests pass there, for 4 passing
// assertions total (1 + 2 + 1). The harness summary counts assertions (like
// qunit-extras' summary, the Node oracle), so it must reproduce
// PASS: 4  FAIL: 0  TOTAL: 4. Without the parent-env inheritance this drops to
// PASS: 1  FAIL: 3  TOTAL: 4 (the inherited-`this` assertions fail).
//
// Expected summary: PASS: 4  FAIL: 0  TOTAL: 4

// A child test sees this.fixture set by the parent module's before hook, even
// though the child module has no hooks of its own.
QUnit.module("parent-before", function (hooks) {
  hooks.before(function () {
    this.fixture = 42;
  });
  QUnit.module("child", function () {
    QUnit.test("child test sees parent before state", function (assert) {
      assert.equal(this.fixture, 42, "inherited this.fixture");
    });
  });
});

// A child module's own before hook sees the parent's before state, and the test
// sees both the inherited value and the child-derived one.
QUnit.module("parent-before-layered", function (hooks) {
  hooks.before(function () {
    this.a = "p";
  });
  QUnit.module("child-layered", function (h2) {
    h2.before(function () {
      this.b = this.a + "-c"; // must observe the parent's this.a
    });
    QUnit.test("child before layers on parent before", function (assert) {
      assert.equal(this.a, "p", "inherited this.a");
      assert.equal(this.b, "p-c", "derived this.b");
    });
  });
});

// A child before hook overrides an inherited value (child wins), matching QUnit.
QUnit.module("parent-before-override", function (hooks) {
  hooks.before(function () {
    this.v = "parent";
  });
  QUnit.module("child-override", function (h3) {
    h3.before(function () {
      this.v = "child";
    });
    QUnit.test("child before overrides parent value", function (assert) {
      assert.equal(this.v, "child", "overridden this.v");
    });
  });
});
