// Self-test for the tape assertion-object adapter. The shared harness is inert
// on Node, so run-harness-selftest.sh validates this fixture on JSSE alone.
//
// Expected summary: PASS: 16  FAIL: 0  TOTAL: 16

var tape = globalThis.__tape;
var restored = { value: true };

tape("tape adapter", function (t) {
  t.equal(1, 1, "strict equality");
  t.notEqual(1, "1", "strict inequality");
  t.deepEqual({ a: [1, NaN] }, { a: [1, NaN] }, "deep equality");
  t.deepEqual([, "x"], [, "x"], "matching sparse array holes");
  t.notDeepEqual([, "x"], [undefined, "x"], "sparse hole is not undefined");
  t.notDeepEqual(
    { a: [, "x"] },
    { a: [undefined, "x"] },
    "nested sparse hole is not undefined"
  );
  t.ok(true, "truthy");
  t.notOk(false, "falsy");

  t.test("nested assertions", function (st) {
    st.plan(4);
    st.throws(function () { throw new TypeError("boom"); }, TypeError, "constructor throw");
    st.throws(function () { throw new RangeError("cycle"); }, /RangeError: cycle/, "regexp throw");
    st.doesNotThrow(function () { return 1; }, "no throw");
    st.match("query-string", /^query-/, "regexp match");
    st.end();
  });

  t.test("interception and teardown", function (st) {
    st.intercept(Object.prototype, "__tapeFixtureValue", { value: 42 });
    st.equal(({}).__tapeFixtureValue, 42, "intercepted inherited value");
    st.teardown(function () { restored.value = true; });
    restored.value = false;
    st.end();
  });

  t.test("intentional skip", { skip: "fixture" }, function () {
    throw new Error("skipped callback ran");
  });

  t.end();
});

tape("prior teardown and restore completed", function (t) {
  t.ok(restored.value, "teardown ran");
  t.equal(typeof ({}).__tapeFixtureValue, "undefined", "intercept restored");
  t.end();
});
